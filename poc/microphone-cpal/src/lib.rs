#![forbid(unsafe_code)]
//! Isolated CPAL microphone capture components for evaluation.

pub mod ring;
pub mod wav;

pub use ring::AudioSampleRing;
pub use wav::{WavWriter, WavWriterError};

use std::sync::Arc;

use cpal::traits::DeviceTrait;
use cpal::{InputCallbackInfo, Stream, StreamConfig};

/// Builds an F32 input stream feeding a preallocated bounded ring buffer.
pub fn build_f32_stream(
    device: &cpal::Device,
    config: StreamConfig,
    ring: Arc<AudioSampleRing>,
    callback_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, String> {
    device
        .build_input_stream(
            config,
            move |samples: &[f32], _info: &InputCallbackInfo| {
                let _ = ring.push_from_callback(samples);
            },
            callback_error,
            None,
        )
        .map_err(|error| error.to_string())
}
