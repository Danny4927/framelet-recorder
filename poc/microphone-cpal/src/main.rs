#![forbid(unsafe_code)]

use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use framelet_capture_api::{AudioBlock, SourceDescriptor, SourceId, SourceKind};
use framelet_microphone_poc::{AudioSampleRing, WavWriter, build_f32_stream};

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
        descriptors.push(SourceDescriptor {
            id: SourceId::new(format!("cpal-input-{index}"))
                .map_err(|error| format!("invalid microphone source id: {error:?}"))?,
            name: format!("Input device {index}"),
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
        record_device(device, parse_duration(&args)?, parse_output(&args)?)?;
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

fn parse_output(args: &[String]) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(position) = args.iter().position(|arg| arg == "--output") {
        let value = args.get(position + 1).ok_or("--output requires a path")?;
        if value.trim().is_empty() {
            return Err("--output path must not be empty".into());
        }
        return Ok(PathBuf::from(value));
    }
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    for suffix in 0_u32..1000 {
        let name = if suffix == 0 {
            format!("framelet-recording-{timestamp}.wav")
        } else {
            format!("framelet-recording-{timestamp}-{suffix}.wav")
        };
        let candidate = PathBuf::from(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("unable to choose a unique default output path".into())
}

fn print_usage() {
    println!("Framelet microphone PoC; writes 32-bit float PCM WAV files.");
    println!("  --list");
    println!("  --record --device INDEX [--duration-seconds N] [--output PATH]");
    println!("Capture starts only after explicit --record. Only default F32 input is accepted.");
}

fn record_device(
    device: &cpal::Device,
    duration: Duration,
    output: PathBuf,
) -> Result<(), Box<dyn Error>> {
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
    let mut writer = WavWriter::create(&output, sample_rate, channels)?;
    if let Err(error) = stream.play() {
        drop(writer);
        let _ = std::fs::remove_file(&output);
        return Err(error.into());
    }

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
            &mut writer,
            sample_rate,
            channels,
            &mut first_sample_position,
            &mut block_count,
        )?;
        std::thread::sleep(CONSUMER_POLL_INTERVAL);
    }
    drop(stream);
    consume_available_blocks(
        &ring,
        &mut scratch,
        &mut writer,
        sample_rate,
        channels,
        &mut first_sample_position,
        &mut block_count,
    )?;

    let dropped_samples = ring.dropped_samples();
    if callback_failed.load(Ordering::Acquire) {
        return Err(
            "audio input stream reported a runtime error (device may have disconnected)".into(),
        );
    }
    if dropped_samples > 0 {
        return Err(format!(
            "audio capture overflowed: {dropped_samples} samples were dropped; output was not finalized"
        )
        .into());
    }
    let completed_path = writer.finalize()?;
    println!(
        "Saved {first_sample_position} sample frames in {block_count} AudioBlocks at {sample_rate} Hz ({channels} channels) to {}.",
        completed_path.display()
    );
    Ok(())
}

fn consume_available_blocks(
    ring: &AudioSampleRing,
    scratch: &mut [f32],
    writer: &mut WavWriter,
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
        let block = AudioBlock::new(
            sample_rate,
            channels,
            timestamp,
            Some(*first_sample_position),
            scratch[..count].to_vec(),
        )
        .map_err(|error| format!("invalid captured audio block: {error:?}"))?;
        writer.write_block(&block)?;
        *first_sample_position = first_sample_position
            .checked_add(u64::try_from(block.frame_count())?)
            .ok_or("audio sample position overflow")?;
        *block_count = block_count.saturating_add(1);
        debug_assert_eq!(block.frame_count(), frames);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn duration_defaults_to_ten_seconds() {
        assert_eq!(
            parse_duration(&args(&[])).expect("default duration"),
            DEFAULT_DURATION
        );
    }

    #[test]
    fn duration_rejects_zero_and_non_numeric_values() {
        assert!(parse_duration(&args(&["--duration-seconds", "0"])).is_err());
        assert!(parse_duration(&args(&["--duration-seconds", "fast"])).is_err());
    }

    #[test]
    fn duration_accepts_positive_integer() {
        assert_eq!(
            parse_duration(&args(&["--duration-seconds", "17"])).expect("duration"),
            Duration::from_secs(17)
        );
    }

    #[test]
    fn output_requires_a_non_empty_path() {
        assert!(parse_output(&args(&["--output"])).is_err());
        assert!(parse_output(&args(&["--output", "  "])).is_err());
        assert_eq!(
            parse_output(&args(&["--output", "capture.wav"])).expect("path"),
            PathBuf::from("capture.wav")
        );
    }

    #[test]
    fn generated_output_path_is_a_candidate_not_a_reserved_file() {
        let path = parse_output(&args(&[])).expect("generated path");
        assert!(path.file_name().is_some());
        assert!(path.to_string_lossy().ends_with(".wav"));
    }
}
