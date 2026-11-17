#![forbid(unsafe_code)]

//! Isolated GStreamer media pipeline PoC.
//!
//! Deterministic hardware-free regression tests live in `validation.rs`.
//!
//! Synthetic owned audio/video blocks are pushed through appsrc elements;
//! native capture adapters and the production workspace are deliberately not
//! involved. This PoC is only a pipeline wiring smoke test until real encoding,
//! muxing, playback, clock synchronization and failure gates are measured.

#[cfg(test)]
mod validation;
#[cfg(test)]
mod media_queue;

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result, bail};
use framelet_capture_api::{AudioBlock, PixelFormat, VideoFrame, VideoFrameMetadata};
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;
const FRAME_RATE: u32 = 30;
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

fn main() -> Result<()> {
    let output = output_path()?;
    let pipeline = build_pipeline(&output)?;

    pipeline
        .set_state(gst::State::Playing)
        .context("starting GStreamer pipeline")?;
    let result = push_test_media(&pipeline).and_then(|()| finish_pipeline(&pipeline));

    if result.is_err() {
        let _ = pipeline.set_state(gst::State::Null);
        let _ = std::fs::remove_file(&output);
    }

    result?;
    println!("Pipeline finalized: {}", output.display());
    Ok(())
}

fn output_path() -> Result<PathBuf> {
    let mut arguments = std::env::args_os().skip(1);
    let path = match (arguments.next(), arguments.next()) {
        (None, None) => std::env::current_dir()?.join("framelet-gstreamer-poc.mkv"),
        (Some(flag), Some(value)) if flag == "--output" => PathBuf::from(value),
        _ => bail!(
            "usage: cargo run --manifest-path poc/media-gstreamer/Cargo.toml -- [--output PATH]"
        ),
    };
    if path.exists() {
        bail!("refusing to overwrite existing output: {}", path.display());
    }
    Ok(path)
}

fn build_pipeline(output: &PathBuf) -> Result<gst::Pipeline> {
    gst::init().context("initializing GStreamer")?;
    let pipeline = gst::Pipeline::new();

    let video_source = gst_app::AppSrc::builder()
        .name("framelet-video-source")
        .caps(
            &gst_video::VideoInfo::builder(gst_video::VideoFormat::Bgra, WIDTH, HEIGHT)
                .fps(gst::Fraction::new(FRAME_RATE as i32, 1))
                .build()
                .context("building video caps")?
                .to_caps()
                .context("converting video caps")?,
        )
        .format(gst::Format::Time)
        .is_live(false)
        .build();

    let video_queue = make_element("queue", "video-queue")?;
    let video_convert = make_element("videoconvert", "video-convert")?;
    let video_encoder = make_element("vp8enc", "video-encoder")?;

    let audio_source = gst_app::AppSrc::builder()
        .name("framelet-audio-source")
        .caps(
            &gst::Caps::builder("audio/x-raw")
                .field("format", "F32LE")
                .field("layout", "interleaved")
                .field("rate", SAMPLE_RATE as i32)
                .field("channels", CHANNELS as i32)
                .build(),
        )
        .format(gst::Format::Time)
        .is_live(false)
        .build();

    let audio_queue = make_element("queue", "audio-queue")?;
    let audio_convert = make_element("audioconvert", "audio-convert")?;
    let audio_resample = make_element("audioresample", "audio-resample")?;
    let audio_encoder = make_element("vorbisenc", "audio-encoder")?;
    let muxer = make_element("matroskamux", "muxer")?;
    let sink = make_element("filesink", "file-sink")?;
    sink.set_property("location", output);
    sink.set_property("sync", false);

    pipeline.add_many([
        video_source.upcast_ref(),
        &video_queue,
        &video_convert,
        &video_encoder,
        audio_source.upcast_ref(),
        &audio_queue,
        &audio_convert,
        &audio_resample,
        &audio_encoder,
        &muxer,
        &sink,
    ])?;

    gst::Element::link_many([
        video_source.upcast_ref(),
        &video_queue,
        &video_convert,
        &video_encoder,
        &muxer,
    ])
    .context("linking video branch")?;

    gst::Element::link_many([
        audio_source.upcast_ref(),
        &audio_queue,
        &audio_convert,
        &audio_resample,
        &audio_encoder,
        &muxer,
    ])
    .context("linking audio branch")?;

    muxer.link(&sink).context("linking Matroska sink")?;
    Ok(pipeline)
}

fn make_element(factory: &str, name: &str) -> Result<gst::Element> {
    gst::ElementFactory::make(factory)
        .name(name)
        .build()
        .with_context(|| format!("creating GStreamer element {factory}"))
}

fn push_test_media(pipeline: &gst::Pipeline) -> Result<()> {
    let video_source = pipeline
        .by_name("framelet-video-source")
        .context("video appsrc missing")?
        .downcast::<gst_app::AppSrc>()
        .map_err(|_| anyhow::anyhow!("video source has unexpected type"))?;

    let audio_source = pipeline
        .by_name("framelet-audio-source")
        .context("audio appsrc missing")?
        .downcast::<gst_app::AppSrc>()
        .map_err(|_| anyhow::anyhow!("audio source has unexpected type"))?;

    let frame_duration = gst::ClockTime::from_nseconds(1_000_000_000 / u64::from(FRAME_RATE));
    let frame_bytes = usize::try_from(WIDTH)
        .ok()
        .and_then(|width| width.checked_mul(usize::try_from(HEIGHT).ok()?))
        .and_then(|pixels| pixels.checked_mul(4))
        .context("video frame size overflow")?;

    for sequence in 0..FRAME_RATE * 5 {
        let timestamp =
            Duration::from_nanos(u64::from(sequence) * (1_000_000_000 / u64::from(FRAME_RATE)));
        let frame = VideoFrame::new(
            VideoFrameMetadata {
                width: WIDTH,
                height: HEIGHT,
                stride: WIDTH.checked_mul(4).context("video stride overflow")?,
                pixel_format: PixelFormat::Bgra8,
                timestamp,
                sequence: u64::from(sequence),
            },
            synthetic_frame(frame_bytes, sequence),
        )
        .map_err(|error| anyhow::anyhow!("creating owned test video frame: {error:?}"))?;

        let mut buffer = gst::Buffer::from_mut_slice(frame.data);
        {
            let buffer_ref = buffer.get_mut().context("video buffer is shared")?;
            buffer_ref.set_pts(gst::ClockTime::from_nseconds(timestamp.as_nanos() as u64));
            buffer_ref.set_duration(frame_duration);
            buffer_ref.set_offset(u64::from(sequence));
        }
        video_source
            .push_buffer(buffer)
            .context("pushing video frame")?;

        if sequence % 3 == 0 {
            let samples_per_channel = (SAMPLE_RATE / FRAME_RATE) * 3;
            let mut samples = Vec::with_capacity(
                usize::try_from(samples_per_channel).unwrap_or_default() * usize::from(CHANNELS),
            );
            for sample in 0..samples_per_channel {
                let t =
                    (sequence * (SAMPLE_RATE / FRAME_RATE) + sample) as f32 / SAMPLE_RATE as f32;
                let value = (t * 440.0 * std::f32::consts::TAU).sin() * 0.1;
                samples.extend([value, value]);
            }
            let block = AudioBlock::new(
                SAMPLE_RATE,
                CHANNELS,
                timestamp,
                Some(u64::from(sequence / 3) * u64::from(samples_per_channel)),
                samples,
            )
            .map_err(|error| anyhow::anyhow!("creating owned test audio block: {error:?}"))?;

            let mut audio_buffer = gst::Buffer::from_mut_slice(f32le_bytes(&block.samples));
            {
                let audio_ref = audio_buffer.get_mut().context("audio buffer is shared")?;
                audio_ref.set_pts(gst::ClockTime::from_nseconds(timestamp.as_nanos() as u64));
                audio_ref.set_duration(gst::ClockTime::from_nseconds(
                    u64::from(samples_per_channel) * 1_000_000_000 / u64::from(SAMPLE_RATE),
                ));
            }
            audio_source
                .push_buffer(audio_buffer)
                .context("pushing audio block")?;
        }
    }

    video_source
        .end_of_stream()
        .context("ending video stream")?;
    audio_source
        .end_of_stream()
        .context("ending audio stream")?;
    Ok(())
}

fn synthetic_frame(length: usize, sequence: u32) -> Vec<u8> {
    let mut data = vec![0; length];
    let (pixels, _remainder) = data.as_chunks_mut::<4>();
    for pixel in pixels {
        pixel[0] = (sequence % 255) as u8;
        pixel[1] = 80;
        pixel[2] = 180;
        pixel[3] = 255;
    }
    data
}

fn f32le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(samples));
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn finish_pipeline(pipeline: &gst::Pipeline) -> Result<()> {
    finish_pipeline_with_timeout(pipeline, Duration::from_secs(30))
}

fn finish_pipeline_with_timeout(pipeline: &gst::Pipeline, timeout: Duration) -> Result<()> {
    let bus = pipeline.bus().context("pipeline bus missing")?;
    let timeout = gst::ClockTime::from_nseconds(
        u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX),
    );
    let message = bus
        .timed_pop_filtered(
            timeout,
            &[gst::MessageType::Eos, gst::MessageType::Error],
        )
        .context("timed out waiting for media finalization")?;

    match message.view() {
        gst::MessageView::Eos(..) => {
            pipeline
                .set_state(gst::State::Null)
                .context("stopping finalized pipeline")?;
            Ok(())
        }
        gst::MessageView::Error(error) => bail!(
            "GStreamer pipeline failed: {} ({:?})",
            error.error(),
            error.debug()
        ),
        _ => bail!("unexpected GStreamer terminal message"),
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn synthetic_audio_video_pipeline_reaches_eos_and_writes_output() {
        let output = std::env::temp_dir().join(format!(
            "framelet-gstreamer-integration-{}-{}.mkv",
            std::process::id(),
            "eos"
        ));
        let _ = std::fs::remove_file(&output);

        let result = (|| {
            let pipeline = build_pipeline(&output)?;
            pipeline
                .set_state(gst::State::Playing)
                .context("starting integration-test pipeline")?;
            push_test_media(&pipeline)?;
            finish_pipeline(&pipeline)?;
            let metadata =
                std::fs::metadata(&output).context("reading finalized integration-test output")?;
            anyhow::ensure!(metadata.len() > 0, "finalized media output is empty");
            Ok::<_, anyhow::Error>(())
        })();

        let _ = std::fs::remove_file(&output);
        result.expect("synthetic GStreamer pipeline should finalize successfully");
    }

    #[test]
    fn missing_output_directory_is_reported_as_pipeline_failure() {
        let output = std::env::temp_dir()
            .join(format!("framelet-gstreamer-missing-{}-{}", std::process::id(), "dir"))
            .join("recording.mkv");
        let _ = std::fs::remove_dir_all(output.parent().expect("parent directory"));
        let pipeline = build_pipeline(&output).expect("pipeline should build");
        pipeline.set_state(gst::State::Playing).expect("pipeline should start");

        let push_result = push_test_media(&pipeline);
        let finish_result = if push_result.is_ok() {
            finish_pipeline_with_timeout(&pipeline, Duration::from_secs(5))
        } else {
            push_result
        };
        let _ = pipeline.set_state(gst::State::Null);
        assert!(finish_result.is_err(), "filesink failure must not report success");
        assert!(!output.exists(), "failed candidate output must not survive");
    }

    #[test]
    fn missing_eos_fails_finalization_after_the_configured_timeout() {
        let output = std::env::temp_dir().join(format!(
            "framelet-gstreamer-timeout-{}.mkv",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&output);
        let pipeline = build_pipeline(&output).expect("pipeline should build");
        pipeline.set_state(gst::State::Playing).expect("pipeline should start");

        let started = std::time::Instant::now();
        let result = finish_pipeline_with_timeout(&pipeline, Duration::from_millis(25));
        let elapsed = started.elapsed();
        let _ = pipeline.set_state(gst::State::Null);
        let _ = std::fs::remove_file(&output);

        assert!(result.is_err(), "missing EOS must fail finalization");
        assert!(elapsed < Duration::from_secs(2), "timeout should be bounded");
    }
}
