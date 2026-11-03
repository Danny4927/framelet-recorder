#![forbid(unsafe_code)]
//! Safe, backend-independent capture contracts.
//!
//! Frames and audio blocks own their data. Adapters must validate native
//! dimensions, strides, and lengths before constructing these values.

use std::time::Duration;

/// Stable identifier assigned by a capture adapter.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SourceId(String);

impl SourceId {
    /// Creates an identifier after checking it is non-empty.
    pub fn new(value: impl Into<String>) -> Result<Self, CaptureError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CaptureError::InvalidSourceId);
        }
        Ok(Self(value))
    }

    /// Returns the identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Kind of source represented by a descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceKind {
    /// User-selected display, monitor, or window.
    Screen,
    /// Camera input.
    Camera,
    /// Microphone input.
    Microphone,
}

/// User-selectable capture source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceDescriptor {
    /// Stable adapter-local identifier.
    pub id: SourceId,
    /// Human-readable display name; not assumed stable or unique.
    pub name: String,
    /// Source category.
    pub kind: SourceKind,
}

/// Supported packed pixel formats at the initial API boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    /// Blue, green, red, alpha; 8 bits per channel.
    Bgra8,
    /// Red, green, blue, alpha; 8 bits per channel.
    Rgba8,
}

impl PixelFormat {
    const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Bgra8 | Self::Rgba8 => 4,
        }
    }
}

/// Metadata for a captured video frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoFrameMetadata {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bytes per row, including any row padding.
    pub stride: u32,
    /// Packed pixel format.
    pub pixel_format: PixelFormat,
    /// Monotonic timestamp relative to the source clock.
    pub timestamp: Duration,
    /// Monotonically increasing sequence number for this source.
    pub sequence: u64,
}

/// Owned video frame. Data length must cover stride multiplied by height.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoFrame {
    /// Validated frame metadata.
    pub metadata: VideoFrameMetadata,
    /// Owned packed pixel bytes.
    pub data: Vec<u8>,
}

impl VideoFrame {
    /// Creates a frame after validating dimensions, stride, and buffer length.
    pub fn new(metadata: VideoFrameMetadata, data: Vec<u8>) -> Result<Self, CaptureError> {
        if metadata.width == 0 || metadata.height == 0 {
            return Err(CaptureError::InvalidVideoDimensions);
        }

        let minimum_stride = metadata
            .width
            .checked_mul(metadata.pixel_format.bytes_per_pixel())
            .ok_or(CaptureError::InvalidVideoLayout)?;

        if metadata.stride < minimum_stride {
            return Err(CaptureError::InvalidVideoLayout);
        }

        let required_len = usize::try_from(metadata.stride)
            .ok()
            .and_then(|stride| {
                usize::try_from(metadata.height)
                    .ok()
                    .and_then(|height| stride.checked_mul(height))
            })
            .ok_or(CaptureError::InvalidVideoLayout)?;

        if data.len() < required_len {
            return Err(CaptureError::VideoBufferTooShort);
        }

        Ok(Self { metadata, data })
    }
}

/// Interleaved floating-point audio samples with one timestamp for the first frame.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBlock {
    /// Sample rate in samples per second.
    pub sample_rate: u32,
    /// Number of interleaved channels.
    pub channels: u16,
    /// Timestamp of the first sample frame on the source clock.
    pub timestamp: Duration,
    /// First-sample position when supplied by the backend.
    pub first_sample_position: Option<u64>,
    /// Interleaved normalized floating-point samples.
    pub samples: Vec<f32>,
}

impl AudioBlock {
    /// Creates an audio block after validating its sample configuration.
    pub fn new(
        sample_rate: u32,
        channels: u16,
        timestamp: Duration,
        first_sample_position: Option<u64>,
        samples: Vec<f32>,
    ) -> Result<Self, CaptureError> {
        if sample_rate == 0 || channels == 0 || samples.len() % usize::from(channels) != 0 {
            return Err(CaptureError::InvalidAudioLayout);
        }

        Ok(Self {
            sample_rate,
            channels,
            timestamp,
            first_sample_position,
            samples,
        })
    }

    /// Number of sample frames (samples per channel).
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }
}

/// Typed capture errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureError {
    /// Source identifier was empty.
    InvalidSourceId,
    /// Source is no longer available.
    SourceUnavailable,
    /// OS denied capture permission.
    PermissionDenied,
    /// User cancelled source picker.
    SelectionCancelled,
    /// Requested format is unsupported.
    UnsupportedFormat,
    /// Video dimensions were zero.
    InvalidVideoDimensions,
    /// Video stride, dimensions, or computed buffer size was invalid/overflowed.
    InvalidVideoLayout,
    /// Video data did not contain all bytes required by its stride and height.
    VideoBufferTooShort,
    /// Audio sample rate, channel count, or interleaved sample count was invalid.
    InvalidAudioLayout,
    /// Backend-specific failure.
    BackendFailure(String),
}

/// Safe lifecycle contract for a video source.
///
/// Implementations are called on a worker, never the UI thread. Streaming
/// delivery and bounded transport are owned by the adapter integration.
pub trait VideoSource {
    /// Lists available sources.
    fn sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError>;
    /// Starts the selected source.
    fn start(&mut self, source: &SourceId) -> Result<(), CaptureError>;
    /// Stops capture and releases the source.
    fn stop(&mut self) -> Result<(), CaptureError>;
}

/// Safe lifecycle contract for a microphone source.
///
/// Implementations must keep device callbacks bounded and non-blocking.
pub trait AudioSource {
    /// Lists available microphone sources.
    fn sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError>;
    /// Starts the selected source.
    fn start(&mut self, source: &SourceId) -> Result<(), CaptureError>;
    /// Stops capture and releases the source.
    fn stop(&mut self) -> Result<(), CaptureError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(width: u32, height: u32, stride: u32) -> VideoFrameMetadata {
        VideoFrameMetadata {
            width,
            height,
            stride,
            pixel_format: PixelFormat::Bgra8,
            timestamp: Duration::from_millis(10),
            sequence: 1,
        }
    }

    #[test]
    fn source_id_rejects_empty_values() {
        assert_eq!(SourceId::new(" "), Err(CaptureError::InvalidSourceId));
    }

    #[test]
    fn source_id_preserves_valid_value() {
        let id = SourceId::new("display-1").expect("valid test identifier");
        assert_eq!(id.as_str(), "display-1");
    }

    #[test]
    fn video_frame_accepts_valid_owned_buffer() {
        let frame = VideoFrame::new(metadata(2, 2, 8), vec![0; 16]);
        assert!(frame.is_ok());
    }

    #[test]
    fn video_frame_rejects_zero_dimensions() {
        assert_eq!(
            VideoFrame::new(metadata(0, 2, 8), vec![0; 16]),
            Err(CaptureError::InvalidVideoDimensions)
        );
    }

    #[test]
    fn video_frame_rejects_short_stride() {
        assert_eq!(
            VideoFrame::new(metadata(2, 2, 7), vec![0; 14]),
            Err(CaptureError::InvalidVideoLayout)
        );
    }

    #[test]
    fn video_frame_rejects_short_buffer() {
        assert_eq!(
            VideoFrame::new(metadata(2, 2, 8), vec![0; 15]),
            Err(CaptureError::VideoBufferTooShort)
        );
    }

    #[test]
    fn audio_block_counts_interleaved_sample_frames() {
        let block = AudioBlock::new(48_000, 2, Duration::ZERO, Some(0), vec![0.0; 960]);
        assert_eq!(block.map(|value| value.frame_count()), Ok(480));
    }

    #[test]
    fn audio_block_rejects_incomplete_interleaved_frame() {
        assert_eq!(
            AudioBlock::new(48_000, 2, Duration::ZERO, None, vec![0.0; 3]),
            Err(CaptureError::InvalidAudioLayout)
        );
    }
}
