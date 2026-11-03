#![forbid(unsafe_code)]

//! Opt-in Wayland portal + PipeWire frame-delivery PoC.
//!
//! This intentionally uses CPU-backed buffers only. It never writes captured
//! pixels to disk and is not part of the production workspace.

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

const DEFAULT_FRAME_COUNT: u64 = 120;
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|argument| argument == "--start") {
        eprintln!("This PoC opens the desktop screen-sharing permission dialog.");
        eprintln!("Run with: cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start [--frames N]");
        return Ok(());
    }

    let frame_limit = parse_frame_limit(&args)?;
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

    // PortalSessionHandle documents that ownership of this FD was transferred
    // to the portal handle at creation and is transferred to PipeWire here.
    // PipeWireThreadManager::new takes exclusive ownership and closes it when
    // its PipeWire core is torn down. No FD duplication or unsafe code here.
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

    println!("PipeWire stream connected. Waiting for CPU-backed frames...");
    let started = Instant::now();
    let mut received = 0_u64;
    let mut rejected = 0_u64;

    while received < frame_limit {
        let Some(frame) = pipewire.recv_frame_timeout(FRAME_TIMEOUT) else {
            let corrupt = pipewire.corrupted_buffer_count();
            return Err(format!(
                "Timed out waiting for frame {}/{}. Corrupted PipeWire buffers: {}.",
                received + 1,
                frame_limit,
                corrupt
            )
            .into());
        };

        match convert_frame(frame) {
            Ok(frame) => {
                received += 1;
                if received == 1 || received.is_multiple_of(30) || received == frame_limit {
                    println!(
                        "Frame {}/{}: {}x{}, stride {}, {:?}, {} bytes, seq {}",
                        received,
                        frame_limit,
                        frame.metadata.width,
                        frame.metadata.height,
                        frame.metadata.stride,
                        frame.metadata.pixel_format,
                        frame.data.len(),
                        frame.metadata.sequence
                    );
                }
            }
            Err(reason) => {
                rejected += 1;
                eprintln!("Rejected PipeWire frame: {reason}");
                if rejected >= 5 {
                    return Err("Five frames were rejected; stopping the PoC.".into());
                }
            }
        }
    }

    println!(
        "Received and validated {received} frames in {:.2}s; rejected {rejected}.",
        started.elapsed().as_secs_f64()
    );
    println!("No recording file was created.");

    pipewire.shutdown()?;
    drop(session);
    Ok(())
}

fn parse_frame_limit(args: &[String]) -> Result<u64, Box<dyn std::error::Error>> {
    let Some(index) = args.iter().position(|argument| argument == "--frames") else {
        return Ok(DEFAULT_FRAME_COUNT);
    };
    let value = args
        .get(index + 1)
        .ok_or("--frames requires a positive integer")?;
    let count = value.parse::<u64>()?;
    if count == 0 {
        return Err("--frames must be greater than zero".into());
    }
    Ok(count)
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
        PipeWirePixelFormat::BGRA => {
            output.copy_from_slice(&source[..required_len]);
        }
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
        other => {
            return Err(format!("unsupported PipeWire pixel format: {other:?}"));
        }
    }

    let metadata = VideoFrameMetadata {
        width,
        height,
        stride: frame.stride,
        pixel_format: FrameletPixelFormat::Bgra8,
        timestamp: Duration::from_nanos(frame.pts),
        sequence: frame.frame_id,
    };
    FrameletVideoFrame::new(metadata, output).map_err(|error| format!("Framelet validation failed: {error:?}"))
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
        for pixel in destination_row.chunks_exact_mut(4) {
            transform(pixel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
