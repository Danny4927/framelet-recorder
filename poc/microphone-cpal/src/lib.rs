#![forbid(unsafe_code)]
//! Isolated CPAL microphone capture experiment.

use std::sync::Arc;
use std::time::Duration;

use cpal::traits::DeviceTrait;

/// Builds an F32 input stream that copies into a preallocated ring buffer.
///
/// The callback performs no allocation, locking, waiting, or I/O.
pub fn build_f32_stream(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    ring: Arc<AudioSampleRing>,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    device.build_input_stream(
        config,
        move |samples: &[f32], _info: &cpal::InputCallbackInfo| {
            ring.push_from_callback(samples);
        },
        on_error,
        Some(Duration::from_secs(1)),
    )
}

pub mod ring;
pub use ring::{AudioSampleRing, RingBufferError};
