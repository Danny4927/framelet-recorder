#![forbid(unsafe_code)]
//! Safe, backend-independent capture contracts.
use std::collections::VecDeque;
use std::collections::VecDeque;
use std::time::Duration;

/// Stable identifier assigned by a capture adapter.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SourceId(String);
impl SourceId {
    /// Creates an identifier after checking it is non-empty.
    pub fn new(value: impl Into<String>) -> Result<Self, CaptureError> {
        let value=value.into();
        if value.trim().is_empty(){return Err(CaptureError::InvalidSourceId);}
        Ok(Self(value))
    }
    /// Returns the identifier as text.
    #[must_use] pub fn as_str(&self)->&str{&self.0}
}
/// Kind of source represented by a descriptor.
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum SourceKind{/// User-selected display, monitor, or window.
Screen,/// Camera input.
Camera,/// Microphone input.
Microphone}
/// User-selectable capture source.
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct SourceDescriptor{/// Stable adapter-local identifier.
pub id:SourceId,/// Human-readable display name; not assumed stable or unique.
pub name:String,/// Source category.
pub kind:SourceKind}
/// Supported packed pixel formats at the initial API boundary.
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum PixelFormat{/// Blue, green, red, alpha; 8 bits per channel.
Bgra8,/// Red, green, blue, alpha; 8 bits per channel.
Rgba8}
impl PixelFormat{const fn bytes_per_pixel(self)->u32{match self{Self::Bgra8|Self::Rgba8=>4}}}
/// Metadata for a captured video frame.
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct VideoFrameMetadata{/// Width in pixels.
pub width:u32,/// Height in pixels.
pub height:u32,/// Bytes per row, including any row padding.
pub stride:u32,/// Packed pixel format.
pub pixel_format:PixelFormat,/// Monotonic timestamp relative to the source clock.
pub timestamp:Duration,/// Monotonically increasing sequence number for this source.
pub sequence:u64}
/// Owned video frame. Data length must cover stride multiplied by height.
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct VideoFrame{/// Validated frame metadata.
pub metadata:VideoFrameMetadata,/// Owned packed pixel bytes.
pub data:Vec<u8>}
impl VideoFrame{
    /// Creates a frame after validating dimensions, stride, and buffer length.
    pub fn new(metadata:VideoFrameMetadata,data:Vec<u8>)->Result<Self,CaptureError>{
        if metadata.width==0||metadata.height==0{return Err(CaptureError::InvalidVideoDimensions);}
        let min=metadata.width.checked_mul(metadata.pixel_format.bytes_per_pixel()).ok_or(CaptureError::InvalidVideoLayout)?;
        if metadata.stride<min{return Err(CaptureError::InvalidVideoLayout);}
        let len=usize::try_from(metadata.stride).ok().and_then(|s|usize::try_from(metadata.height).ok().and_then(|h|s.checked_mul(h))).ok_or(CaptureError::InvalidVideoLayout)?;
        if data.len()<len{return Err(CaptureError::VideoBufferTooShort);}
        Ok(Self{metadata,data})
    }
}
/// Interleaved floating-point audio samples with one timestamp for the first frame.
#[derive(Clone,Debug,PartialEq)]
pub struct AudioBlock{/// Sample rate in samples per second.
pub sample_rate:u32,/// Number of interleaved channels.
pub channels:u16,/// Timestamp of the first sample frame on the source clock.
pub timestamp:Duration,/// First-sample position when supplied by the backend.
pub first_sample_position:Option<u64>,/// Interleaved normalized floating-point samples.
pub samples:Vec<f32>}
impl AudioBlock{
    /// Creates an audio block after validating its sample configuration and samples.
    pub fn new(sample_rate:u32,channels:u16,timestamp:Duration,first_sample_position:Option<u64>,samples:Vec<f32>)->Result<Self,CaptureError>{
        if sample_rate==0||channels==0||!samples.len().is_multiple_of(usize::from(channels)){return Err(CaptureError::InvalidAudioLayout);}
        if samples.iter().any(|s|!s.is_finite()){return Err(CaptureError::InvalidAudioSample);}
        Ok(Self{sample_rate,channels,timestamp,first_sample_position,samples})
    }
    /// Number of sample frames (samples per channel).
    #[must_use] pub fn frame_count(&self)->usize{self.samples.len()/usize::from(self.channels)}
    /// Returns true when the block contains no sample frames.
    #[must_use] pub fn is_empty(&self)->bool{self.samples.is_empty()}
}
/// Errors produced while validating capture data or transporting it.
#[derive(Clone,Debug,Eq,PartialEq)]
pub enum CaptureError{/// Source identifier was empty.
InvalidSourceId,/// Source is no longer available.
SourceUnavailable,/// OS denied capture permission.
PermissionDenied,/// User cancelled source picker.
SelectionCancelled,/// Requested format is unsupported.
UnsupportedFormat,/// Video dimensions were zero.
InvalidVideoDimensions,/// Video stride, dimensions, or computed buffer size was invalid/overflowed.
InvalidVideoLayout,/// Video data did not contain all bytes required by its stride and height.
VideoBufferTooShort,/// Audio sample rate, channel count, or interleaved sample count was invalid.
InvalidAudioLayout,/// Audio contains a NaN or infinite sample.
InvalidAudioSample,/// An audio block did not match the stream's negotiated format.
AudioFormatMismatch,/// An audio block would exceed the configured maximum size.
AudioBlockTooLarge,/// The audio transport has no available capacity.
AudioTransportFull,/// Backend-specific failure.
BackendFailure(String)}
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
    pub fn len(&self) -> usize { self.blocks.len() }

    /// Returns whether no blocks are queued.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.blocks.is_empty() }

    /// Returns the configured block capacity.
    #[must_use]
    pub const fn capacity_blocks(&self) -> usize { self.capacity_blocks }

    /// Returns the maximum interleaved samples in one block.
    #[must_use]
    pub const fn max_block_samples(&self) -> usize { self.max_block_samples }

    /// Returns blocks rejected because the queue was full.
    #[must_use]
    pub const fn rejected_blocks(&self) -> u64 { self.rejected_blocks }
}

/// Safe lifecycle contract for a video source.
pub trait VideoSource{/// Lists available sources.
fn sources(&self)->Result<Vec<SourceDescriptor>,CaptureError>;/// Starts the selected source.
fn start(&mut self,source:&SourceId)->Result<(),CaptureError>;/// Stops capture and releases the source.
fn stop(&mut self)->Result<(),CaptureError>}
/// Safe lifecycle contract for a microphone source.
pub trait AudioSource{/// Lists available microphone sources.
fn sources(&self)->Result<Vec<SourceDescriptor>,CaptureError>;/// Starts the selected source.
fn start(&mut self,source:&SourceId)->Result<(),CaptureError>;/// Stops capture and releases the source.
fn stop(&mut self)->Result<(),CaptureError>}
#[cfg(test)]
mod tests{
use super::*;
fn metadata(width:u32,height:u32,stride:u32)->VideoFrameMetadata{VideoFrameMetadata{width,height,stride,pixel_format:PixelFormat::Bgra8,timestamp:Duration::from_millis(10),sequence:1}}
fn audio(samples:Vec<f32>)->AudioBlock{AudioBlock::new(48_000,2,Duration::ZERO,Some(0),samples).expect("valid test audio")}
#[test]fn source_id_rejects_empty_values(){assert_eq!(SourceId::new(" "),Err(CaptureError::InvalidSourceId));}
#[test]fn source_id_preserves_valid_value(){let id=SourceId::new("display-1").expect("valid test identifier");assert_eq!(id.as_str(),"display-1");}
#[test]fn video_frame_accepts_valid_owned_buffer(){assert!(VideoFrame::new(metadata(2,2,8),vec![0;16]).is_ok());}
#[test]fn video_frame_rejects_zero_dimensions(){assert_eq!(VideoFrame::new(metadata(0,2,8),vec![0;16]),Err(CaptureError::InvalidVideoDimensions));}
#[test]fn video_frame_rejects_short_stride(){assert_eq!(VideoFrame::new(metadata(2,2,7),vec![0;14]),Err(CaptureError::InvalidVideoLayout));}
#[test]fn video_frame_rejects_short_buffer(){assert_eq!(VideoFrame::new(metadata(2,2,8),vec![0;15]),Err(CaptureError::VideoBufferTooShort));}
#[test]fn audio_block_counts_interleaved_sample_frames(){let b=audio(vec![0.0;960]);assert_eq!(b.frame_count(),480);assert!(!b.is_empty());}
#[test]fn audio_block_rejects_incomplete_interleaved_frame(){assert_eq!(AudioBlock::new(48_000,2,Duration::ZERO,None,vec![0.0;3]),Err(CaptureError::InvalidAudioLayout));}
#[test]fn audio_block_rejects_non_finite_samples(){assert_eq!(AudioBlock::new(48_000,2,Duration::ZERO,None,vec![0.0,f32::NAN]),Err(CaptureError::InvalidAudioSample));assert_eq!(AudioBlock::new(48_000,2,Duration::ZERO,None,vec![f32::INFINITY,0.0]),Err(CaptureError::InvalidAudioSample));}
#[test]fn audio_queue_is_fifo_and_bounded(){let mut q=AudioBlockQueue::new(2,8,48_000,2).expect("queue config");q.try_push(audio(vec![1.0,1.0])).expect("first");q.try_push(AudioBlock::new(48_000,2,Duration::from_millis(1),Some(1),vec![2.0,2.0]).expect("second")).expect("second");assert_eq!(q.try_push(audio(vec![3.0,3.0])),Err(CaptureError::AudioTransportFull));assert_eq!(q.len(),2);assert_eq!(q.rejected_blocks(),1);assert_eq!(q.pop().map(|b|b.samples),Some(vec![1.0,1.0]));assert_eq!(q.pop().map(|b|b.samples),Some(vec![2.0,2.0]));assert!(q.is_empty());}
#[test]fn audio_queue_rejects_format_mismatch_and_oversized_blocks(){let mut q=AudioBlockQueue::new(1,2,48_000,2).expect("queue config");let mismatch=AudioBlock::new(44_100,2,Duration::ZERO,None,vec![0.0,0.0]).expect("valid");assert_eq!(q.try_push(mismatch),Err(CaptureError::AudioFormatMismatch));assert_eq!(q.try_push(audio(vec![0.0,0.0,0.0,0.0])),Err(CaptureError::AudioBlockTooLarge));assert_eq!(q.len(),0);assert_eq!(q.rejected_blocks(),0);}
}    #[test]
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
        queue.try_push(AudioBlock::new(48_000, 2, Duration::ZERO, Some(0), vec![1.0, 1.0])
            .expect("first block")).expect("first push");
        queue.try_push(AudioBlock::new(48_000, 2, Duration::from_millis(1), Some(1), vec![2.0, 2.0])
            .expect("second block")).expect("second push");

        assert_eq!(
            queue.try_push(AudioBlock::new(48_000, 2, Duration::from_millis(2), Some(2), vec![3.0, 3.0])
                .expect("third block")),
            Err(CaptureError::AudioTransportFull)
        );
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.rejected_blocks(), 1);
        assert_eq!(queue.pop().map(|block| block.samples), Some(vec![1.0, 1.0]));
        assert_eq!(queue.pop().map(|block| block.samples), Some(vec![2.0, 2.0]));
        assert!(queue.is_empty());
    }

    #[test]
    fn audio_queue_rejects_format_mismatch_and_oversized_blocks() {
        let mut queue = AudioBlockQueue::new(1, 2, 48_000, 2).expect("valid queue");
        let wrong_rate = AudioBlock::new(44_100, 2, Duration::ZERO, None, vec![0.0, 0.0])
            .expect("valid block");
        assert_eq!(queue.try_push(wrong_rate), Err(CaptureError::AudioFormatMismatch));

        let oversized = AudioBlock::new(48_000, 2, Duration::ZERO, None, vec![0.0; 4])
            .expect("valid block");
        assert_eq!(queue.try_push(oversized), Err(CaptureError::AudioBlockTooLarge));
        assert_eq!(queue.len(), 0);
        assert_eq!(queue.rejected_blocks(), 0);
    }

    #[test]
    fn audio_queue_rejects_zero_capacities_and_formats() {
        assert_eq!(AudioBlockQueue::new(0, 8, 48_000, 2).err(), Some(CaptureError::InvalidAudioLayout));
        assert_eq!(AudioBlockQueue::new(1, 0, 48_000, 2).err(), Some(CaptureError::InvalidAudioLayout));
        assert_eq!(AudioBlockQueue::new(1, 8, 0, 2).err(), Some(CaptureError::InvalidAudioLayout));
        assert_eq!(AudioBlockQueue::new(1, 8, 48_000, 0).err(), Some(CaptureError::InvalidAudioLayout));
    }


