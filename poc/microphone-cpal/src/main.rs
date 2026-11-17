#![forbid(unsafe_code)]

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use framelet_capture_api::{AudioBlock, SourceDescriptor, SourceId, SourceKind};

use framelet_microphone_poc::{AudioSampleRing, build_f32_stream};

const DEFAULT_DURATION: Duration = Duration::from_secs(10);
const RING_SECONDS: usize = 2;
const CONSUMER_CHUNK_MILLIS: u64 = 100;
const CONSUMER_POLL_INTERVAL: Duration = Duration::from_millis(10);

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return Ok(());
    }
    if !args.iter().any(|arg| arg == "--list") && !args.iter().any(|arg| arg == "--record") {
        print_usage();
        return Ok(());
    }

    let host = cpal::default_host();
    let mut descriptors = Vec::new();
    let mut devices = Vec::new();

    for (index, device) in host.input_devices()?.enumerate() {
        let name = format!("Input device {index}");
        descriptors.push(SourceDescriptor {
            id: SourceId::new(format!("cpal-input-{index}"))
                .map_err(|error| format!("invalid source identifier: {error:?}"))?,
            name,
            kind: SourceKind::Microphone,
        });
        devices.push(device);
    }

    if args.iter().any(|arg| arg == "--list") {
        for (index, source) in descriptors.iter().enumerate() {
            println!("{index}: {} ({})", source.name, source.id.as_str());
        }
        if descriptors.is_empty() {
            println!("No microphone input devices found.");
        }
    }

    if args.iter().any(|arg| arg == "--record") {
        let index = args
            .iter()
            .position(|arg| arg == "--device")
            .and_then(|position| args.get(position + 1))
            .ok_or("--record requires --device INDEX")?
            .parse::<usize>()?;
        let device = devices
            .get(index)
            .ok_or("microphone index is out of range")?;
        record_device(device, parse_duration(&args)?)?;
    }

    Ok(())
}

fn parse_duration(args: &[String]) -> Result<Duration, Box<dyn Error>> {
    let seconds = args
        .iter()
        .position(|arg| arg == "--duration-seconds")
        .and_then(|position| args.get(position + 1))
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(DEFAULT_DURATION.as_secs());
    if seconds == 0 {
        return Err("--duration-seconds must be greater than zero".into());
    }
    Ok(Duration::from_secs(seconds))
}

fn print_usage() {
    println!("Framelet microphone PoC; it does not write files.");
    println!("  --list");
    println!("  --record --device INDEX [--duration-seconds N]");
    println!("The input stream starts only after explicit --record.");
    println!("Only F32 input is accepted; bounded AudioBlocks are consumed and discarded on exit.");
}

fn record_device(device: &cpal::Device, duration: Duration) -> Result<(), Box<dyn Error>> {
    let supported = device.default_input_config()?;
    if supported.sample_format() != SampleFormat::F32 {
        return Err(format!("unsupported sample format: {:?}", supported.sample_format()).into());
    }

    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let channels_usize = usize::from(channels);
    if sample_rate == 0 || channels == 0 {
        return Err("input device reported an invalid audio configuration".into());
    }

    let ring_capacity = usize::try_from(sample_rate)
        .ok()
        .and_then(|rate| rate.checked_mul(channels_usize))
        .and_then(|samples| samples.checked_mul(RING_SECONDS))
        .ok_or("ring capacity overflow")?;

    let ring = Arc::new(
        AudioSampleRing::new(ring_capacity)
            .map_err(|error| format!("unable to allocate audio ring: {error:?}"))?,
    );
    let callback_failed = Arc::new(AtomicBool::new(false));
    let callback_error = Arc::clone(&callback_failed);
    let config: StreamConfig = supported.into();

    let stream = build_f32_stream(
        device,
        config,
        Arc::clone(&ring),
        move |_error: cpal::Error| {
            callback_error.store(true, Ordering::Release);
        },
    )?;

    // CPAL 0.18 returns streams paused on all platforms; start explicitly.
    stream.play()?;

    // Drain continuously while the stream is live. The callback remains a
    // bounded, non-blocking producer; all allocation/API work stays here.
    let chunk_frames = (u64::from(sample_rate) * CONSUMER_CHUNK_MILLIS / 1_000).max(1);
    let chunk_capacity = usize::try_from(chunk_frames)
        .ok()
        .and_then(|frames| frames.checked_mul(channels_usize))
        .ok_or("consumer chunk size overflow")?;
    let mut scratch = vec![0.0_f32; chunk_capacity];
    let started = Instant::now();
    let mut first_sample_position = 0_u64;
    let mut block_count = 0_u64;

    while started.elapsed() < duration && !callback_failed.load(Ordering::Acquire) {
        consume_available_blocks(
            &ring,
            &mut scratch,
            sample_rate,
            channels,
            &mut first_sample_position,
            &mut block_count,
        )?;
        std::thread::sleep(CONSUMER_POLL_INTERVAL);
    }

    // Stop the producer before the final drain so no samples are left behind.
    drop(stream);
    consume_available_blocks(
        &ring,
        &mut scratch,
        sample_rate,
        channels,
        &mut first_sample_position,
        &mut block_count,
    )?;

    println!(
        "Consumed {} sample frames in {} AudioBlocks at {} Hz ({} channels); {} samples dropped. No file created.",
        first_sample_position,
        block_count,
        sample_rate,
        channels,
        ring.dropped_samples()
    );

    if callback_failed.load(Ordering::Acquire) {
        return Err("audio input stream reported a runtime error (device may have disconnected)".into());
    }
    Ok(())
}

/// Converts each drained chunk into an owned API block and immediately hands
/// it to the PoC sink. The sink intentionally discards data until a writer is
/// implemented; the bounded chunk keeps consumer memory independent of duration.
fn consume_available_blocks(
    ring: &AudioSampleRing,
    scratch: &mut [f32],
    sample_rate: u32,
    channels: u16,
    first_sample_position: &mut u64,
    block_count: &mut u64,
) -> Result<(), Box<dyn Error>> {
    loop {
        let count = ring.pop_into(scratch);
        if count == 0 {
            return Ok(());
        }
        if !count.is_multiple_of(usize::from(channels)) {
            return Err("audio ring returned an incomplete interleaved frame".into());
        }

        let frames = count / usize::from(channels);
        let timestamp =
            Duration::from_secs_f64(*first_sample_position as f64 / f64::from(sample_rate));
        let samples = scratch[..count].to_vec();
        let block = AudioBlock::new(
            sample_rate,
            channels,
            timestamp,
            Some(*first_sample_position),
            samples,
        )
        .map_err(|error| format!("invalid audio block: {error:?}"))?;

        // This is the seam for the future bounded encoder/writer channel.
        // Dropping the owned block is deliberate in this capture-only PoC.
        *first_sample_position = first_sample_position
            .checked_add(u64::try_from(block.frame_count())?)
            .ok_or("audio sample position overflow")?;
        *block_count = block_count.saturating_add(1);
        debug_assert_eq!(block.frame_count(), frames);
        drop(block);
    }
}
