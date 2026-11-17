#![forbid(unsafe_code)]
//! Isolated CPAL microphone capture components for evaluation.

pub mod ring;
pub mod wav;

pub use ring::AudioSampleRing;
pub use wav::{WavWriter, WavWriterError};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use cpal::traits::DeviceTrait;
use cpal::{Data, InputCallbackInfo, Stream, StreamConfig};

/// Builds an F32 input stream feeding a preallocated bounded ring buffer.
pub fn build_f32_stream(
    device: &cpal::Device,
    config: StreamConfig,
    ring: Arc<AudioSampleRing>,
    callback_error: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream, cpal::BuildStreamError> {
    let callback_failed = Arc::new(AtomicBool::new(false));
    let callback_failed_data = Arc::clone(&callback_failed);
    device.build_input_stream(
        &config,
        move |data: &Data, _info: &InputCallbackInfo| {
            if let Some(samples) = data.as_slice::<f32>() {
                let _ = ring.push_from_callback(samples);
            } else {
                callback_failed_data.store(true, Ordering::Release);
            }
        },
        callback_error,
        None,
    )
}
