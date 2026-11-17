#![forbid(unsafe_code)]

//! Opt-in Wayland portal + PipeWire frame-delivery and lifecycle PoC.
//!
//! CPU-backed frames are validated and discarded; this is not a recorder and
//! never writes captured pixels or media to disk.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use framelet_capture_api::{
    PixelFormat as FrameletPixelFormat, VideoFrame as FrameletVideoFrame,
    VideoFrameMetadata,
};
use lamco_pipewire::{
    PipeWireThreadCommand, PipeWireThreadManager, PixelFormat as PipeWirePixelFormat,
    StreamConfig,
};
use lamco_wayland::prelude::*;

const DEFAULT_DURATION: Duration = Duration::from_secs(600);
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const REPORT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_REJECTED_FRAMES: u64 = 5;

#[derive(Debug, Clone, Copy)]
struct Options {
    frame_limit: Option<u64>,
    duration: Duration,
}

#[derive(Debug, Default)]
struct CaptureStats {
    received: u64,
    rejected: u64,
    last_sequence: Option<u64>,
    sequence_gaps: u64,
    first_timestamp: Option<Duration>,
    last_timestamp: Option<Duration>,
    timestamp_regressions: u64,
    max_frame_bytes: usize,
    total_frame_bytes: u128,
}

impl CaptureStats {
    fn record(&mut self, frame: &FrameletVideoFrame) {
        self.received = self.received.saturating_add(1);
        self.max_frame_bytes = self.max_frame_bytes.max(frame.data.len());
        self.total_frame_bytes = self
            .total_frame_bytes
            .saturating_add(frame.data.len() as u128);

        if let Some(previous) = self.last_sequence
            && frame.metadata.sequence > previous.saturating_add(1)
        {
            self.sequence_gaps = self.sequence_gaps.saturating_add(
                frame.metadata.sequence.saturating_sub(previous).saturating_sub(1),
            );
        }
        self.last_sequence = Some(frame.metadata.sequence);

        if let Some(previous) = self.last_timestamp {
            if frame.metadata.timestamp < previous {
                self.timestamp_regressions = self.timestamp_regressions.saturating_add(1);
            }
        } else {
            self.first_timestamp = Some(frame.metadata.timestamp);
        }
        self.last_timestamp = Some(frame.metadata.timestamp);
    }

    fn report(&self, elapsed: Duration, rejected_buffers: u64, label: &str) {
        let seconds = elapsed.as_secs_f64().max(f64::EPSILON);
        let fps = self.received as f64 / seconds;
        println!(
            "{label}: {:.1}s, {} validated, {} rejected, {:.1} received FPS,              {} sequence gaps, {} timestamp regressions, {} corrupt PipeWire buffers,              {} MiB owned frame bytes (max frame {} KiB)",
            elapsed.as_secs_f64(),
            self.received,
            self.rejected,
            fps,
            self.sequence_gaps,
            self.timestamp_regressions,
            rejected_buffers,
            self.total_frame_bytes / (1024 * 1024),
            self.max_frame_bytes / 1024,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopReason {
    FrameTargetReached,
    DurationReached,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|argument| argument == "--start") {
        print_usage();
        return Ok(());
    }

    let options = parse_options(&args)?;
    let portal_manager = PortalManager::with_default().await?;
    let (session, _restore_token) = portal_manager
        .create_session("framelet-wayland-poc".to_owned(), None)
        .await?;

    let Some(stream_info) = session.streams().first().cloned() else {
        return Err("The portal returned no capture streams.".into());
    };

    println!(
        "Portal session created. Selected stream node {} ({}x{}).",
        stream_info.node_id, stream_info.size.0, stream_info.size.1
    );

    // PipeWireThreadManager takes ownership of the portal-provided FD.
    // The portal session is retained until the PipeWire thread is shut down.
    let mut pipewire = PipeWireThreadManager::new(session.pipewire_fd())?;
    let config = StreamConfig::new("framelet-screen-poc")
        .with_resolution(stream_info.size.0, stream_info.size.1)
        .with_framerate(30)
        .with_dmabuf(false)
        .with_buffer_count(3);

    let (response_tx, response_rx) = mpsc::sync_channel(1);
    pipewire.send_command(PipeWireThreadCommand::CreateStream {
        stream_id: 0,
        node_id: stream_info.node_id,
        config,
        response_tx,
    })?;
    response_rx.recv()??;

    println!(
        "PipeWire stream connected. Waiting for frames (duration: {:.0}s, frame target: {}).",
        options.duration.as_secs_f64(),
        options.frame_limit.map_or_else(|| "none".to_owned(), |n| n.to_string()),
    );

    let started = Instant::now();
    let mut last_report = started;
    let mut stats = CaptureStats::default();
    let stop_reason = loop {
        let elapsed = started.elapsed();
        if elapsed >= options.duration {
            break StopReason::DurationReached;
        }
        if options.frame_limit.is_some_and(|limit| stats.received >= limit) {
            break StopReason::FrameTargetReached;
        }

        let remaining = options.duration.saturating_sub(elapsed);
        let wait = FRAME_TIMEOUT.min(remaining);
        let Some(pipewire_frame) = pipewire.recv_frame_timeout(wait) else {
            if started.elapsed() >= options.duration {
                break StopReason::DurationReached;
            }
            let corrupt = pipewire.corrupted_buffer_count();
            let error = format!(
                "Timed out waiting for frame after {} validated frames.                  Corrupted PipeWire buffers: {corrupt}.",
                stats.received
            );
            return shutdown_with_error(pipewire, session, error);
        };

        match convert_frame(pipewire_frame) {
            Ok(frame) => stats.record(&frame),
            Err(reason) => {
                stats.rejected = stats.rejected.saturating_add(1);
                eprintln!("Rejected PipeWire frame: {reason}");
                if stats.rejected >= MAX_REJECTED_FRAMES {
                    return shutdown_with_error(
                        pipewire,
                        session,
                        format!("{} frames were rejected; stopping the PoC.", stats.rejected),
                    );
                }
            }
        }

        if last_report.elapsed() >= REPORT_INTERVAL {
            stats.report(started.elapsed(), pipewire.corrupted_buffer_count(), "Progress");
            last_report = Instant::now();
        }
    };

    let elapsed = started.elapsed();
    let corrupt = pipewire.corrupted_buffer_count();
    stats.report(elapsed, corrupt, "Final");
    println!("Stop reason: {stop_reason:?}. No recording file was created.");

    pipewire.shutdown()?;
    drop(session);
    println!("PipeWire thread shut down and portal session released.");
    Ok(())
}

fn shutdown_with_error(
    mut pipewire: PipeWireThreadManager,
    session: PortalSessionHandle,
    message: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let shutdown_result = pipewire.shutdown();
    drop(session);
    match shutdown_result {
        Ok(()) => Err(message.into()),
        Err(shutdown_error) => Err(format!(
            "{message} PipeWire shutdown also failed: {shutdown_error}"
        )
        .into()),
    }
}

fn print_usage() {
    eprintln!("This PoC opens the desktop screen-sharing permission dialog.");
    eprintln!(
        "Run with: cargo run --manifest-path poc/wayland-portal/Cargo.toml --          --start [--frames N] [--duration-seconds N]"
    );
    eprintln!("Defaults: 120 frames or 600 seconds, whichever occurs first.");
}

fn parse_options(args: &[String]) -> Result<Options, Box<dyn std::error::Error>> {
    let frame_limit = parse_positive_option(args, "--frames")?;
    let duration_seconds = parse_positive_option(args, "--duration-seconds")?;
    let duration = Duration::from_secs(duration_seconds.unwrap_or(DEFAULT_DURATION.as_secs()));
    Ok(Options {
        frame_limit,
        duration,
    })
}

fn parse_positive_option(
    args: &[String],
    option: &str,
) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    let Some(index) = args.iter().position(|argument| argument == option) else {
        return Ok(None);
    };
    let value = args
        .get(index + 1)
        .ok_or_else(|| format!("{option} requires a positive integer"))?;
    let parsed = value.parse::<u64>()?;
    if parsed == 0 {
        return Err(format!("{option} must be greater than zero").into());
    }
    Ok(Some(parsed))
}

fn convert_frame(
    frame: lamco_pipewire::VideoFrame,
) -> Result<FrameletVideoFrame, String> {
    if frame.is_dmabuf() {
        return Err("DMA-BUF frame received although CPU buffers were requested".to_owned());
    }
    if frame.flags.is_corrupted() || frame.flags.is_incomplete() {
        return Err("PipeWire marked the frame corrupted or incomplete".to_owned());
    }

    let width = frame.width;
    let height = frame.height;
    if width == 0 || height == 0 {
        return Err("zero-sized frame".to_owned());
    }

    let minimum_stride = width
        .checked_mul(4)
        .ok_or_else(|| "pixel row size overflow".to_owned())?;
    if frame.stride < minimum_stride {
        return Err(format!(
            "unsupported packed pixel stride {} (minimum {})",
            frame.stride, minimum_stride
        ));
    }

    let required_len = usize::try_from(frame.stride)
        .ok()
        .and_then(|stride| {
            usize::try_from(height)
                .ok()
                .and_then(|height| stride.checked_mul(height))
        })
        .ok_or_else(|| "frame buffer size overflow".to_owned())?;

    let source = frame
        .data()
        .ok_or_else(|| "frame has no CPU-resident pixel data".to_owned())?;
    if source.len() < required_len {
        return Err(format!(
            "short pixel buffer: {} bytes, expected at least {}",
            source.len(),
            required_len
        ));
    }

    let mut output = vec![0_u8; required_len];
    match frame.format {
        PipeWirePixelFormat::BGRA => output.copy_from_slice(&source[..required_len]),
        PipeWirePixelFormat::BGRx => {
            copy_bgrx_to_bgra(source, &mut output, width, height, frame.stride)?;
        }
        PipeWirePixelFormat::RGBA | PipeWirePixelFormat::RGBx => {
            copy_rgbx_to_bgra(
                source,
                &mut output,
                width,
                height,
                frame.stride,
                frame.format == PipeWirePixelFormat::RGBA,
            )?;
        }
        other => return Err(format!("unsupported PipeWire pixel format: {other:?}")),
    }

    let metadata = VideoFrameMetadata {
        width,
        height,
        stride: frame.stride,
        pixel_format: FrameletPixelFormat::Bgra8,
        timestamp: Duration::from_nanos(frame.pts),
        sequence: frame.frame_id,
    };
    FrameletVideoFrame::new(metadata, output)
        .map_err(|error| format!("Framelet validation failed: {error:?}"))
}

fn copy_bgrx_to_bgra(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    stride: u32,
) -> Result<(), String> {
    copy_rows(source, destination, width, height, stride, |pixel| {
        pixel[3] = 255;
    })
}

fn copy_rgbx_to_bgra(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    stride: u32,
    has_alpha: bool,
) -> Result<(), String> {
    copy_rows(source, destination, width, height, stride, |pixel| {
        pixel.swap(0, 2);
        if !has_alpha {
            pixel[3] = 255;
        }
    })
}

fn copy_rows(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    stride: u32,
    mut transform: impl FnMut(&mut [u8]),
) -> Result<(), String> {
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| "row byte count overflow".to_owned())?;
    let stride = usize::try_from(stride).map_err(|_| "stride does not fit usize".to_owned())?;

    for row in 0..usize::try_from(height).map_err(|_| "height does not fit usize".to_owned())? {
        let row_start = row
            .checked_mul(stride)
            .ok_or_else(|| "row offset overflow".to_owned())?;
        let row_end = row_start
            .checked_add(row_bytes)
            .ok_or_else(|| "row end overflow".to_owned())?;
        let source_row = source
            .get(row_start..row_end)
            .ok_or_else(|| "source row is outside the frame buffer".to_owned())?;
        let destination_row = destination
            .get_mut(row_start..row_end)
            .ok_or_else(|| "destination row is outside the frame buffer".to_owned())?;

        destination_row.copy_from_slice(source_row);
        for pixel in destination_row.as_chunks_mut::<4>().0 {
            transform(pixel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults() {
        let parsed = parse_options(&["poc".to_owned(), "--start".to_owned()])
            .expect("valid defaults");
        assert_eq!(parsed.frame_limit, None);
        assert_eq!(parsed.duration, Duration::from_secs(600));
    }

    #[test]
    fn parses_both_limits() {
        let parsed = parse_options(&[
            "poc".to_owned(),
            "--frames".to_owned(),
            "120".to_owned(),
            "--duration-seconds".to_owned(),
            "10".to_owned(),
        ])
        .expect("valid limits");
        assert_eq!(parsed.frame_limit, Some(120));
        assert_eq!(parsed.duration, Duration::from_secs(10));
    }

    #[test]
    fn rejects_zero_duration_or_frame_limit() {
        for args in [
            vec!["poc".to_owned(), "--frames".to_owned(), "0".to_owned()],
            vec![
                "poc".to_owned(),
                "--duration-seconds".to_owned(),
                "0".to_owned(),
            ],
        ] {
            assert!(parse_options(&args).is_err());
        }
    }

    #[test]
    fn bgrx_becomes_bgra_and_sets_opaque_alpha() {
        let mut destination = [0_u8; 4];
        copy_bgrx_to_bgra(&[1, 2, 3, 99], &mut destination, 1, 1, 4)
            .expect("valid one-pixel frame");
        assert_eq!(destination, [1, 2, 3, 255]);
    }

    #[test]
    fn rgba_becomes_bgra() {
        let mut destination = [0_u8; 4];
        copy_rgbx_to_bgra(&[10, 20, 30, 40], &mut destination, 1, 1, 4, true)
            .expect("valid one-pixel frame");
        assert_eq!(destination, [30, 20, 10, 40]);
    }

    #[test]
    fn rgbx_becomes_bgra_with_opaque_alpha() {
        let mut destination = [0_u8; 4];
        copy_rgbx_to_bgra(&[10, 20, 30, 99], &mut destination, 1, 1, 4, false)
            .expect("valid one-pixel frame");
        assert_eq!(destination, [30, 20, 10, 255]);
    }

    #[test]
    fn row_padding_is_preserved() {
        let source = [1, 2, 3, 4, 5, 6, 7, 8, 9, 9, 9, 9];
        let mut destination = [0_u8; 12];
        copy_bgrx_to_bgra(&source, &mut destination, 2, 1, 12)
            .expect("valid padded frame");
        assert_eq!(destination, [1, 2, 3, 255, 5, 6, 7, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn stats_count_frame_sequence_gaps_and_timestamp_regressions() {
        let mut stats = CaptureStats::default();
        let frame = |sequence, timestamp| FrameletVideoFrame {
            metadata: VideoFrameMetadata {
                width: 1,
                height: 1,
                stride: 4,
                pixel_format: FrameletPixelFormat::Bgra8,
                timestamp: Duration::from_nanos(timestamp),
                sequence,
            },
            data: vec![0; 4],
        };
        stats.record(&frame(1, 10));
        stats.record(&frame(3, 20));
        stats.record(&frame(4, 15));
        assert_eq!(stats.received, 3);
        assert_eq!(stats.sequence_gaps, 1);
        assert_eq!(stats.timestamp_regressions, 1);
        assert_eq!(stats.max_frame_bytes, 4);
        assert_eq!(stats.total_frame_bytes, 12);
    }
}
