#![forbid(unsafe_code)]
//! Safe, backend-independent capture contracts.
//!
//! Frames and audio blocks own their data. Adapters must validate native
//! dimensions, strides, and lengths before constructing these values.

use std::collections::VecDeque;
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
    /// Creates an audio block after validating its sample configuration and finite samples.
    pub fn new(
        sample_rate: u32,
        channels: u16,
        timestamp: Duration,
        first_sample_position: Option<u64>,
        samples: Vec<f32>,
    ) -> Result<Self, CaptureError> {
        if sample_rate == 0 || channels == 0 || !samples.len().is_multiple_of(usize::from(channels))
        {
            return Err(CaptureError::InvalidAudioLayout);
        }

        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(CaptureError::InvalidAudioSample);
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
    /// An audio sample was NaN or infinite.
    InvalidAudioSample,
    /// Audio block format differs from the queue's negotiated format.
    AudioFormatMismatch,
    /// Audio block exceeds the configured maximum sample count.
    AudioBlockTooLarge,
    /// Bounded audio queue is full; no queued audio was discarded.
    AudioTransportFull,
    /// Backend-specific failure.
    BackendFailure(String),
}

/// A fixed-capacity FIFO for owned audio blocks transferred between workers.
///
/// This queue may allocate and is not real-time safe. Never call it from an
/// audio device callback; callbacks should write to a preallocated SPSC ring
/// that a consumer worker drains into owned AudioBlocks.
#[derive(Debug)]
pub struct AudioBlockQueue {
    blocks: VecDeque<AudioBlock>,
    capacity_blocks: usize,
    max_block_samples: usize,
    sample_rate: u32,
    channels: u16,
    rejected_blocks: u64,
}

impl AudioBlockQueue {
    /// Creates a bounded queue for one negotiated audio format.
    pub fn new(
        capacity_blocks: usize,
        max_block_samples: usize,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Self, CaptureError> {
        if capacity_blocks == 0 || max_block_samples == 0 || sample_rate == 0 || channels == 0 {
            return Err(CaptureError::InvalidAudioLayout);
        }
        Ok(Self {
            blocks: VecDeque::with_capacity(capacity_blocks),
            capacity_blocks,
            max_block_samples,
            sample_rate,
            channels,
            rejected_blocks: 0,
        })
    }

    /// Enqueues a block, rejecting mismatches, oversized blocks, or a full queue.
    ///
    /// A full queue rejects the newest block and retains all previously accepted
    /// blocks. Callers must surface this result as an explicit audio overrun.
    pub fn try_push(&mut self, block: AudioBlock) -> Result<(), CaptureError> {
        if block.sample_rate != self.sample_rate || block.channels != self.channels {
            return Err(CaptureError::AudioFormatMismatch);
        }
        if block.samples.len() > self.max_block_samples {
            return Err(CaptureError::AudioBlockTooLarge);
        }
        if self.blocks.len() >= self.capacity_blocks {
            self.rejected_blocks = self.rejected_blocks.saturating_add(1);
            return Err(CaptureError::AudioTransportFull);
        }
        self.blocks.push_back(block);
        Ok(())
    }

    /// Removes the oldest queued block.
    pub fn pop(&mut self) -> Option<AudioBlock> {
        self.blocks.pop_front()
    }

    /// Returns the number of queued blocks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Returns whether no blocks are queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Returns the configured block capacity.
    #[must_use]
    pub const fn capacity_blocks(&self) -> usize {
        self.capacity_blocks
    }

    /// Returns the maximum interleaved samples in one block.
    #[must_use]
    pub const fn max_block_samples(&self) -> usize {
        self.max_block_samples
    }

    /// Returns blocks rejected because the queue was full.
    #[must_use]
    pub const fn rejected_blocks(&self) -> u64 {
        self.rejected_blocks
    }
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

    fn frame_metadata(
        width: u32,
        height: u32,
        stride: u32,
        pixel_format: PixelFormat,
    ) -> VideoFrameMetadata {
        VideoFrameMetadata {
            pixel_format,
            ..metadata(width, height, stride)
        }
    }

    /// Independent reference predicate computed in `u128`, so it cannot overflow.
    fn reference_result(meta: &VideoFrameMetadata, len: usize) -> Result<(), CaptureError> {
        if meta.width == 0 || meta.height == 0 {
            return Err(CaptureError::InvalidVideoDimensions);
        }
        let bytes_per_pixel: u128 = match meta.pixel_format {
            PixelFormat::Bgra8 | PixelFormat::Rgba8 => 4,
        };
        let minimum_stride = u128::from(meta.width) * bytes_per_pixel;
        if minimum_stride > u128::from(u32::MAX) || u128::from(meta.stride) < minimum_stride {
            return Err(CaptureError::InvalidVideoLayout);
        }
        let required = u128::from(meta.stride) * u128::from(meta.height);
        let addressable = u128::try_from(usize::MAX).unwrap_or(u128::MAX);
        if required > addressable {
            return Err(CaptureError::InvalidVideoLayout);
        }
        let len = u128::try_from(len).unwrap_or(u128::MAX);
        if len < required {
            return Err(CaptureError::VideoBufferTooShort);
        }
        Ok(())
    }

    /// Runs the real constructor with a real buffer of `len` bytes.
    fn constructor_result(meta: &VideoFrameMetadata, len: usize) -> Result<(), CaptureError> {
        VideoFrame::new(meta.clone(), vec![0; len]).map(|_| ())
    }

    /// Required buffer length in `u128`, when the layout itself is valid.
    fn reference_required_len(meta: &VideoFrameMetadata) -> Option<u128> {
        reference_result(meta, usize::MAX)
            .ok()
            .map(|()| u128::from(meta.stride) * u128::from(meta.height))
    }

    #[test]
    fn video_frame_rejects_every_zero_dimension_combination() {
        for (width, height) in [(0, 0), (0, 1), (1, 0), (0, u32::MAX), (u32::MAX, 0)] {
            assert_eq!(
                VideoFrame::new(metadata(width, height, u32::MAX), vec![0; 64]),
                Err(CaptureError::InvalidVideoDimensions)
            );
        }
    }

    #[test]
    fn video_frame_rejects_width_whose_minimum_stride_overflows_u32() {
        // 4 * (u32::MAX / 4 + 1) overflows u32.
        let width = u32::MAX / 4 + 1;
        assert_eq!(
            VideoFrame::new(metadata(width, 1, u32::MAX), Vec::new()),
            Err(CaptureError::InvalidVideoLayout)
        );
        assert_eq!(
            VideoFrame::new(metadata(u32::MAX, u32::MAX, u32::MAX), Vec::new()),
            Err(CaptureError::InvalidVideoLayout)
        );
    }

    #[test]
    fn video_frame_rejects_oversized_dimensions_without_allocating() {
        // Largest representable layout: the required length is either not
        // addressable (32-bit) or far larger than the empty buffer (64-bit).
        let result = VideoFrame::new(metadata(u32::MAX / 4, u32::MAX, u32::MAX), Vec::new());
        #[cfg(target_pointer_width = "64")]
        assert_eq!(result, Err(CaptureError::VideoBufferTooShort));
        #[cfg(not(target_pointer_width = "64"))]
        assert_eq!(result, Err(CaptureError::InvalidVideoLayout));
    }

    #[test]
    fn video_frame_accepts_padded_stride_and_longer_buffer() {
        assert!(VideoFrame::new(metadata(2, 2, 12), vec![0; 24]).is_ok());
        assert!(VideoFrame::new(metadata(2, 2, 8), vec![0; 17]).is_ok());
        assert_eq!(
            VideoFrame::new(metadata(2, 2, 12), vec![0; 23]),
            Err(CaptureError::VideoBufferTooShort)
        );
    }

    #[test]
    fn video_frame_validates_every_supported_pixel_format() {
        // `PixelFormat` is a closed enum: unsupported formats are unrepresentable
        // at this boundary, so adapters must map unknown native formats to
        // `CaptureError::UnsupportedFormat` before constructing a frame.
        for format in [PixelFormat::Bgra8, PixelFormat::Rgba8] {
            assert!(VideoFrame::new(frame_metadata(3, 1, 12, format), vec![0; 12]).is_ok());
            assert_eq!(
                VideoFrame::new(frame_metadata(3, 1, 11, format), vec![0; 12]),
                Err(CaptureError::InvalidVideoLayout)
            );
        }
    }

    #[test]
    fn video_frame_matches_reference_exhaustively_for_small_layouts() {
        for format in [PixelFormat::Bgra8, PixelFormat::Rgba8] {
            for width in 0..=4 {
                for height in 0..=4 {
                    for stride in 0..=20 {
                        for len in 0..=80 {
                            let meta = frame_metadata(width, height, stride, format);
                            assert_eq!(
                                constructor_result(&meta, len),
                                reference_result(&meta, len),
                                "{meta:?} len={len}"
                            );
                        }
                    }
                }
            }
        }
    }

    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn pick_u32(&mut self) -> u32 {
            const EDGES: [u32; 10] = [
                0,
                1,
                2,
                3,
                4,
                u32::MAX / 4,
                u32::MAX / 4 + 1,
                u32::MAX - 1,
                u32::MAX,
                1 << 16,
            ];
            let value = self.next();
            let low = u32::try_from(value & 0xFFFF_FFFF).unwrap_or(0);
            match value >> 60 {
                0..=5 => EDGES[usize::try_from(value % 10).unwrap_or(0)],
                6..=10 => low % 64,
                _ => low,
            }
        }

        fn pick_len(&mut self) -> usize {
            let value = self.next();
            match value >> 61 {
                0 => 0,
                1 => usize::MAX,
                2 => usize::MAX - 1,
                3 | 4 => usize::try_from(value % 4096).unwrap_or(0),
                _ => usize::try_from(value % (1 << 40)).unwrap_or(usize::MAX),
            }
        }
    }

    #[test]
    fn video_frame_matches_reference_for_seeded_random_layouts() {
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        for _ in 0..200_000 {
            let format = if rng.next() & 1 == 0 {
                PixelFormat::Bgra8
            } else {
                PixelFormat::Rgba8
            };
            let meta = frame_metadata(rng.pick_u32(), rng.pick_u32(), rng.pick_u32(), format);
            const ALLOCATION_LIMIT: u128 = 1 << 20;
            match reference_required_len(&meta) {
                Some(required) if required <= ALLOCATION_LIMIT => {
                    let random = u128::from(rng.next() % 4096);
                    let candidates = [required.saturating_sub(1), required, required + 1, random];
                    for candidate in candidates {
                        let len = usize::try_from(candidate).unwrap_or(0);
                        assert_eq!(
                            constructor_result(&meta, len),
                            reference_result(&meta, len),
                            "{meta:?} len={len}"
                        );
                    }
                }
                _ => {
                    // Too large to allocate: the real constructor with an empty
                    // buffer must report the exact reference error.
                    let len = rng.pick_len();
                    let expected = reference_result(&meta, 0);
                    assert!(expected.is_err(), "{meta:?}");
                    assert_eq!(constructor_result(&meta, 0), expected, "{meta:?}");
                    // Arbitrary huge lengths only change the outcome for valid
                    // layouts; the reference must stay panic-free for them.
                    let _ = reference_result(&meta, len);
                }
            }
        }
    }

    #[test]
    fn audio_block_counts_interleaved_sample_frames() {
        let block = AudioBlock::new(48_000, 2, Duration::ZERO, Some(0), vec![0.0; 960]);
        assert_eq!(block.map(|value| value.frame_count()), Ok(480));
    }

    #[test]
    fn audio_block_rejects_non_finite_samples() {
        assert_eq!(
            AudioBlock::new(48_000, 2, Duration::ZERO, None, vec![f32::NAN, 0.0]),
            Err(CaptureError::InvalidAudioSample)
        );
        assert_eq!(
            AudioBlock::new(48_000, 2, Duration::ZERO, None, vec![f32::INFINITY, 0.0]),
            Err(CaptureError::InvalidAudioSample)
        );
    }

    #[test]
    fn audio_queue_preserves_fifo_and_rejects_newest_when_full() {
        let mut queue = AudioBlockQueue::new(2, 8, 48_000, 2).expect("valid queue");
        let first = AudioBlock::new(48_000, 2, Duration::ZERO, Some(0), vec![1.0, 1.0])
            .expect("first block");
        let second = AudioBlock::new(48_000, 2, Duration::from_millis(1), Some(1), vec![2.0, 2.0])
            .expect("second block");
        queue.try_push(first).expect("first push");
        queue.try_push(second).expect("second push");

        let third = AudioBlock::new(48_000, 2, Duration::from_millis(2), Some(2), vec![3.0, 3.0])
            .expect("third block");
        assert_eq!(queue.try_push(third), Err(CaptureError::AudioTransportFull));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.rejected_blocks(), 1);
        assert_eq!(queue.pop().map(|block| block.samples), Some(vec![1.0, 1.0]));
        assert_eq!(queue.pop().map(|block| block.samples), Some(vec![2.0, 2.0]));
        assert!(queue.is_empty());
    }

    #[test]
    fn audio_queue_rejects_format_mismatch_and_oversized_blocks() {
        let mut queue = AudioBlockQueue::new(1, 2, 48_000, 2).expect("valid queue");
        let wrong_rate =
            AudioBlock::new(44_100, 2, Duration::ZERO, None, vec![0.0, 0.0]).expect("valid block");
        assert_eq!(
            queue.try_push(wrong_rate),
            Err(CaptureError::AudioFormatMismatch)
        );

        let oversized =
            AudioBlock::new(48_000, 2, Duration::ZERO, None, vec![0.0; 4]).expect("valid block");
        assert_eq!(
            queue.try_push(oversized),
            Err(CaptureError::AudioBlockTooLarge)
        );
        assert_eq!(queue.len(), 0);
        assert_eq!(queue.rejected_blocks(), 0);
    }

    #[test]
    fn audio_queue_rejects_invalid_configuration() {
        assert_eq!(
            AudioBlockQueue::new(0, 8, 48_000, 2).err(),
            Some(CaptureError::InvalidAudioLayout)
        );
        assert_eq!(
            AudioBlockQueue::new(1, 0, 48_000, 2).err(),
            Some(CaptureError::InvalidAudioLayout)
        );
        assert_eq!(
            AudioBlockQueue::new(1, 8, 0, 2).err(),
            Some(CaptureError::InvalidAudioLayout)
        );
        assert_eq!(
            AudioBlockQueue::new(1, 8, 48_000, 0).err(),
            Some(CaptureError::InvalidAudioLayout)
        );
    }

    #[test]
    fn audio_block_rejects_incomplete_interleaved_frame() {
        assert_eq!(
            AudioBlock::new(48_000, 2, Duration::ZERO, None, vec![0.0; 3]),
            Err(CaptureError::InvalidAudioLayout)
        );
    }
}
