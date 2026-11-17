#![forbid(unsafe_code)]

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use framelet_capture_api::{SourceDescriptor, SourceId, SourceKind};

use framelet_microphone_poc::{AudioSampleRing, build_f32_stream};

const DEFAULT_DURATION: Duration = Duration::from_secs(10);
const RING_SECONDS: usize = 2;

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
            id: SourceId::new(format!("cpal-input-{index}")).map_err(|error| format!("invalid source identifier: {error:?}"))?,
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
        let device = devices.get(index).ok_or("microphone index is out of range")?;
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
    println!("Only F32 input is accepted; samples are discarded on exit.");
}

fn record_device(device: &cpal::Device, duration: Duration) -> Result<(), Box<dyn Error>> {
    let supported = device.default_input_config()?;
    if supported.sample_format() != SampleFormat::F32 {
        return Err(format!("unsupported sample format: {:?}", supported.sample_format()).into());
    }

    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let ring_capacity = usize::try_from(sample_rate)
        .ok()
        .and_then(|rate| rate.checked_mul(usize::from(channels)))
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

    let started = Instant::now();
    while started.elapsed() < duration && !callback_failed.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(stream);

    let chunk_capacity = usize::try_from(sample_rate)
        .map_err(|_| "sample rate does not fit this platform")?
        .checked_mul(usize::from(channels))
        .ok_or("drain buffer size overflow")?
        .max(1);
    let mut scratch = vec![0.0_f32; chunk_capacity.min(ring_capacity)];
    let mut sample_count = 0_u64;
    loop {
        let count = ring.pop_into(&mut scratch);
        if count == 0 {
            break;
        }
        let count = u64::try_from(count).map_err(|_| "sample count overflow")?;
        sample_count = sample_count.saturating_add(count);
    }

    println!(
        "Captured {} sample frames at {} Hz ({} channels); {} samples dropped. No file created.",
        sample_count / u64::from(channels),
        sample_rate,
        channels,
        ring.dropped_samples()
    );

    if callback_failed.load(Ordering::Acquire) {
        return Err("audio input stream reported a runtime error".into());
    }
    Ok(())
}
