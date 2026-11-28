#![forbid(unsafe_code)]
//! Data plane of the Recording bounded context (ADR-0020).
//!
//! Media payloads travel from a capture adapter to the media pipeline through
//! core-owned, bounded, single-producer/single-consumer channels. The media
//! pipeline creates them in [`crate::ports::MediaPipelinePort::prepare`] with
//! [`media_inputs`], keeps the receiving [`MediaStreams`], and hands the
//! sending [`MediaSinks`] to the application, which passes each sink to the
//! capture adapter it starts. Capture and media adapters therefore never know
//! each other, and the overload policy is enforced by this module rather than
//! re-implemented by every adapter:
//!
//! - **Video:** a full queue discards its *oldest* (stale) frame to admit the
//!   newest one and counts the drop ([`VideoDelivery::DroppedStale`]).
//! - **Audio:** a full queue *rejects* the newest block, keeps every accepted
//!   block, counts the loss, and marks the next accepted block with an explicit
//!   [`AudioGap`] ([`AudioDelivery::Discontinuity`]). Audio loss is never silent.
//! - **Source loss:** the capture adapter consumes its sink with
//!   `report_source_lost`; the consumer sees [`StreamEnd::SourceLost`] and the
//!   application observes it through a [`StreamMonitor`].
//!
//! Queue occupancy can never exceed the capacity fixed at construction.
//! Sinks are meant for capture *workers*: `try_send` never blocks on a full
//! queue, but it takes a short internal lock and must not be called from a
//! real-time audio device callback (see ADR-0009).

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use framelet_capture_api::{AudioBlock, VideoFrame};

use crate::domain::{RecordingConfiguration, SourceKind};

/// Hard ceiling for a video queue, in frames. A sanity bound, not a sizing
/// recommendation: real capacities are measured per backend (Phase D).
pub const MAX_VIDEO_QUEUE_CAPACITY: usize = 64;

/// Hard ceiling for an audio queue, in blocks. A sanity bound, not a sizing
/// recommendation.
pub const MAX_AUDIO_QUEUE_CAPACITY: usize = 1024;

/// Typed data-plane construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataPlaneError {
    /// A queue capacity was zero or above the allowed maximum.
    InvalidCapacity {
        /// Capacity that was requested.
        requested: usize,
        /// Largest capacity allowed for this stream type.
        maximum: usize,
    },
}

impl fmt::Display for DataPlaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapacity { requested, maximum } => write!(
                f,
                "media queue capacity {requested} is outside 1..={maximum}"
            ),
        }
    }
}

impl std::error::Error for DataPlaneError {}

/// Why a stream ended. Recorded once, when the producing sink goes away.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamEnd {
    /// The capture adapter released its sink (normal stop or failed start).
    Stopped,
    /// The capture adapter reported that its source disappeared.
    SourceLost,
}

/// Audio that was lost before the packet that carries this marker.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AudioGap {
    /// Number of rejected audio blocks.
    pub lost_blocks: u64,
    /// Number of rejected sample frames (samples per channel).
    pub lost_frames: u64,
}

/// An accepted audio block, explicitly marked if audio was lost before it.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioPacket {
    /// The accepted block.
    pub block: AudioBlock,
    /// Audio rejected between the previous accepted block and this one.
    pub gap_before: Option<AudioGap>,
}

/// Outcome of offering a video frame to a [`VideoSink`].
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoDelivery {
    /// The frame was queued; nothing was dropped.
    Accepted,
    /// The frame was queued after the oldest queued frame was discarded.
    DroppedStale {
        /// Total frames this stream has dropped so far.
        dropped_total: u64,
    },
    /// The consumer released its stream; the frame was discarded.
    Closed,
}

/// Outcome of offering an audio block to an [`AudioSink`].
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioDelivery {
    /// The block was queued.
    Accepted,
    /// The queue was full; the block was rejected and a gap will be marked.
    Discontinuity {
        /// Total blocks this stream has rejected so far.
        lost_blocks_total: u64,
    },
    /// The consumer released its stream; the block was discarded.
    Closed,
}

/// Result of a receive attempt on a stream.
#[derive(Clone, Debug, PartialEq)]
pub enum Received<T> {
    /// The oldest queued item.
    Item(T),
    /// Nothing is queued yet and the producer is still active.
    Empty,
    /// Nothing is queued and the producer has gone; no further items arrive.
    Ended(StreamEnd),
}

/// Counters of one stream. A consistent snapshot taken under the stream lock.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StreamStats {
    /// Fixed queue capacity.
    pub capacity: usize,
    /// Items currently queued; never greater than `capacity`.
    pub queued: usize,
    /// Largest number of items that were ever queued at once.
    pub high_water: usize,
    /// Items admitted into the queue.
    pub accepted: u64,
    /// Items taken by the consumer.
    pub delivered: u64,
    /// Video: stale frames discarded. Audio: blocks rejected.
    pub dropped: u64,
    /// Audio sample frames rejected (zero for video).
    pub lost_audio_frames: u64,
    /// Audio gap episodes (consecutive rejections count once; zero for video).
    pub discontinuities: u64,
    /// Audio rejected since the last accepted block, not yet marked on any
    /// packet. Once the stream has ended this is *trailing* loss that no
    /// packet will ever carry: the consumer must read it from the final
    /// `stats()` and treat it like a marked gap (ADR-0020).
    pub pending_gap: Option<AudioGap>,
    /// How the producer ended, once it has.
    pub end: Option<StreamEnd>,
    /// Whether the consumer released its stream.
    pub consumer_closed: bool,
}

struct State<T> {
    queue: VecDeque<T>,
    high_water: usize,
    accepted: u64,
    delivered: u64,
    dropped: u64,
    lost_audio_frames: u64,
    discontinuities: u64,
    pending_gap: Option<AudioGap>,
    end: Option<StreamEnd>,
    consumer_closed: bool,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    available: Condvar,
    capacity: usize,
}

impl<T> Shared<T> {
    fn new(capacity: usize, maximum: usize) -> Result<Arc<Self>, DataPlaneError> {
        if capacity == 0 || capacity > maximum {
            return Err(DataPlaneError::InvalidCapacity {
                requested: capacity,
                maximum,
            });
        }
        Ok(Arc::new(Self {
            state: Mutex::new(State {
                queue: VecDeque::with_capacity(capacity),
                high_water: 0,
                accepted: 0,
                delivered: 0,
                dropped: 0,
                lost_audio_frames: 0,
                discontinuities: 0,
                pending_gap: None,
                end: None,
                consumer_closed: false,
            }),
            available: Condvar::new(),
            capacity,
        }))
    }

    /// Locks the state. A poisoned lock only follows a panic elsewhere; the
    /// counters stay meaningful, so recover the data instead of panicking.
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn admit(&self, state: &mut State<T>, item: T) {
        state.queue.push_back(item);
        state.accepted = state.accepted.saturating_add(1);
        state.high_water = state.high_water.max(state.queue.len());
    }

    fn stats(&self) -> StreamStats {
        let state = self.lock();
        StreamStats {
            capacity: self.capacity,
            queued: state.queue.len(),
            high_water: state.high_water,
            accepted: state.accepted,
            delivered: state.delivered,
            dropped: state.dropped,
            lost_audio_frames: state.lost_audio_frames,
            discontinuities: state.discontinuities,
            pending_gap: state.pending_gap,
            end: state.end,
            consumer_closed: state.consumer_closed,
        }
    }

    fn end(&self, end: StreamEnd) {
        let mut state = self.lock();
        if state.end.is_none() {
            state.end = Some(end);
        }
        drop(state);
        self.available.notify_all();
    }

    fn take(state: &mut State<T>) -> Received<T> {
        match state.queue.pop_front() {
            Some(item) => {
                state.delivered = state.delivered.saturating_add(1);
                Received::Item(item)
            }
            None => state.end.map_or(Received::Empty, Received::Ended),
        }
    }

    fn try_recv(&self) -> Received<T> {
        Self::take(&mut self.lock())
    }

    fn recv_timeout(&self, timeout: Duration) -> Received<T> {
        let guard = self.lock();
        let (mut guard, _) = self
            .available
            .wait_timeout_while(guard, timeout, |state| {
                state.queue.is_empty() && state.end.is_none()
            })
            .unwrap_or_else(PoisonError::into_inner);
        Self::take(&mut guard)
    }

    fn close_consumer(&self) {
        let mut state = self.lock();
        state.consumer_closed = true;
        // Release queued payloads now; the producer may live on until stopped.
        let discarded = std::mem::take(&mut state.queue);
        drop(state);
        drop(discarded);
    }
}

/// Read-only view of one stream's counters and end state.
///
/// The application keeps a monitor for every sink it hands to a capture
/// adapter, so it can observe source loss and overload without touching the
/// payload path.
#[derive(Clone)]
pub struct StreamMonitor(Monitored);

#[derive(Clone)]
enum Monitored {
    Video(Arc<Shared<VideoFrame>>),
    Audio(Arc<Shared<AudioPacket>>),
}

impl StreamMonitor {
    /// Returns a consistent snapshot of the stream counters.
    #[must_use]
    pub fn stats(&self) -> StreamStats {
        match &self.0 {
            Monitored::Video(shared) => shared.stats(),
            Monitored::Audio(shared) => shared.stats(),
        }
    }

    /// Returns whether the producer reported that its source was lost.
    #[must_use]
    pub fn source_lost(&self) -> bool {
        self.stats().end == Some(StreamEnd::SourceLost)
    }
}

impl fmt::Debug for StreamMonitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("StreamMonitor").field(&self.stats()).finish()
    }
}

/// Producing end of a bounded video stream, owned by one capture adapter.
pub struct VideoSink {
    shared: Arc<Shared<VideoFrame>>,
}

impl VideoSink {
    /// Offers a frame without blocking. A full queue discards its oldest frame.
    pub fn try_send(&mut self, frame: VideoFrame) -> VideoDelivery {
        let mut state = self.shared.lock();
        if state.consumer_closed {
            drop(state);
            drop(frame);
            return VideoDelivery::Closed;
        }
        let mut stale = None;
        let mut delivery = VideoDelivery::Accepted;
        if state.queue.len() >= self.shared.capacity {
            stale = state.queue.pop_front();
            state.dropped = state.dropped.saturating_add(1);
            delivery = VideoDelivery::DroppedStale {
                dropped_total: state.dropped,
            };
        }
        self.shared.admit(&mut state, frame);
        drop(state);
        drop(stale);
        self.shared.available.notify_one();
        delivery
    }

    /// Ends the stream because the source disappeared. Consumes the sink, so
    /// nothing can be sent after the loss.
    pub fn report_source_lost(self) {
        self.shared.end(StreamEnd::SourceLost);
    }

    /// Returns a monitor for this stream.
    #[must_use]
    pub fn monitor(&self) -> StreamMonitor {
        StreamMonitor(Monitored::Video(Arc::clone(&self.shared)))
    }

    /// Returns the fixed queue capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.shared.capacity
    }
}

impl Drop for VideoSink {
    fn drop(&mut self) {
        self.shared.end(StreamEnd::Stopped);
    }
}

impl fmt::Debug for VideoSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("VideoSink")
            .field(&self.shared.stats())
            .finish()
    }
}

/// Producing end of a bounded audio stream, owned by one capture adapter.
pub struct AudioSink {
    shared: Arc<Shared<AudioPacket>>,
}

impl AudioSink {
    /// Offers a block without blocking. A full queue rejects the block, keeps
    /// all accepted audio, and marks an explicit gap before the next block.
    pub fn try_send(&mut self, block: AudioBlock) -> AudioDelivery {
        let mut state = self.shared.lock();
        if state.consumer_closed {
            drop(state);
            drop(block);
            return AudioDelivery::Closed;
        }
        if state.queue.len() >= self.shared.capacity {
            let frames = u64::try_from(block.frame_count()).unwrap_or(u64::MAX);
            state.dropped = state.dropped.saturating_add(1);
            state.lost_audio_frames = state.lost_audio_frames.saturating_add(frames);
            if state.pending_gap.is_none() {
                state.discontinuities = state.discontinuities.saturating_add(1);
            }
            let gap = state.pending_gap.get_or_insert_with(AudioGap::default);
            gap.lost_blocks = gap.lost_blocks.saturating_add(1);
            gap.lost_frames = gap.lost_frames.saturating_add(frames);
            let delivery = AudioDelivery::Discontinuity {
                lost_blocks_total: state.dropped,
            };
            drop(state);
            drop(block);
            return delivery;
        }
        let gap_before = state.pending_gap.take();
        self.shared
            .admit(&mut state, AudioPacket { block, gap_before });
        drop(state);
        self.shared.available.notify_one();
        AudioDelivery::Accepted
    }

    /// Ends the stream because the source disappeared. Consumes the sink, so
    /// nothing can be sent after the loss.
    pub fn report_source_lost(self) {
        self.shared.end(StreamEnd::SourceLost);
    }

    /// Returns a monitor for this stream.
    #[must_use]
    pub fn monitor(&self) -> StreamMonitor {
        StreamMonitor(Monitored::Audio(Arc::clone(&self.shared)))
    }

    /// Returns the fixed queue capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.shared.capacity
    }
}

impl Drop for AudioSink {
    fn drop(&mut self) {
        self.shared.end(StreamEnd::Stopped);
    }
}

impl fmt::Debug for AudioSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AudioSink")
            .field(&self.shared.stats())
            .finish()
    }
}

/// Consuming end of a bounded video stream, owned by the media pipeline.
pub struct VideoStream {
    shared: Arc<Shared<VideoFrame>>,
}

impl VideoStream {
    /// Takes the oldest frame without waiting.
    pub fn try_recv(&mut self) -> Received<VideoFrame> {
        self.shared.try_recv()
    }

    /// Waits up to `timeout` for a frame or the end of the stream.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Received<VideoFrame> {
        self.shared.recv_timeout(timeout)
    }

    /// Returns a snapshot of the stream counters.
    #[must_use]
    pub fn stats(&self) -> StreamStats {
        self.shared.stats()
    }
}

impl Drop for VideoStream {
    fn drop(&mut self) {
        self.shared.close_consumer();
    }
}

impl fmt::Debug for VideoStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("VideoStream")
            .field(&self.shared.stats())
            .finish()
    }
}

/// Consuming end of a bounded audio stream, owned by the media pipeline.
pub struct AudioStream {
    shared: Arc<Shared<AudioPacket>>,
}

impl AudioStream {
    /// Takes the oldest packet without waiting.
    pub fn try_recv(&mut self) -> Received<AudioPacket> {
        self.shared.try_recv()
    }

    /// Waits up to `timeout` for a packet or the end of the stream.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Received<AudioPacket> {
        self.shared.recv_timeout(timeout)
    }

    /// Returns a snapshot of the stream counters.
    ///
    /// Consumer obligation: after [`Received::Ended`], check
    /// [`StreamStats::pending_gap`]. Audio rejected after the last accepted
    /// block is reported only there, because no later packet can carry it.
    #[must_use]
    pub fn stats(&self) -> StreamStats {
        self.shared.stats()
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        self.shared.close_consumer();
    }
}

impl fmt::Debug for AudioStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AudioStream")
            .field(&self.shared.stats())
            .finish()
    }
}

/// Creates a bounded video channel with `capacity` frames.
pub fn video_channel(capacity: usize) -> Result<(VideoSink, VideoStream), DataPlaneError> {
    let shared = Shared::new(capacity, MAX_VIDEO_QUEUE_CAPACITY)?;
    Ok((
        VideoSink {
            shared: Arc::clone(&shared),
        },
        VideoStream { shared },
    ))
}

/// Creates a bounded audio channel with `capacity` blocks.
pub fn audio_channel(capacity: usize) -> Result<(AudioSink, AudioStream), DataPlaneError> {
    let shared = Shared::new(capacity, MAX_AUDIO_QUEUE_CAPACITY)?;
    Ok((
        AudioSink {
            shared: Arc::clone(&shared),
        },
        AudioStream { shared },
    ))
}

/// Queue capacities chosen by a media pipeline for its input streams.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamCapacities {
    /// Capacity of each video stream, in frames.
    pub video: usize,
    /// Capacity of the audio stream, in blocks.
    pub audio: usize,
}

/// Producing ends for exactly the sources a recording configuration requires.
///
/// Only [`media_inputs`] creates this value, so its set of sinks always
/// matches *some* configuration; the application still checks it against the
/// session's configuration with [`MediaSinks::matches`].
#[derive(Debug)]
pub struct MediaSinks {
    screen: Option<VideoSink>,
    camera: Option<VideoSink>,
    microphone: Option<AudioSink>,
}

impl MediaSinks {
    /// Returns whether a sink is (still) present for `kind`.
    #[must_use]
    pub const fn has(&self, kind: SourceKind) -> bool {
        match kind {
            SourceKind::Screen => self.screen.is_some(),
            SourceKind::Camera => self.camera.is_some(),
            SourceKind::Microphone => self.microphone.is_some(),
        }
    }

    /// Returns whether the sinks are exactly those `configuration` requires.
    #[must_use]
    pub fn matches(&self, configuration: &RecordingConfiguration) -> bool {
        [
            SourceKind::Screen,
            SourceKind::Camera,
            SourceKind::Microphone,
        ]
        .into_iter()
        .all(|kind| self.has(kind) == configuration.requires_source(kind))
    }

    /// Takes the video sink for a screen or camera source.
    pub const fn take_video(&mut self, kind: SourceKind) -> Option<VideoSink> {
        match kind {
            SourceKind::Screen => self.screen.take(),
            SourceKind::Camera => self.camera.take(),
            SourceKind::Microphone => None,
        }
    }

    /// Takes the microphone sink.
    pub const fn take_audio(&mut self) -> Option<AudioSink> {
        self.microphone.take()
    }
}

/// Consuming ends kept by the media pipeline, one per required source.
#[derive(Debug, Default)]
pub struct MediaStreams {
    /// Screen video, when the configuration records a screen.
    pub screen: Option<VideoStream>,
    /// Camera video, when the configuration records a camera.
    pub camera: Option<VideoStream>,
    /// Microphone audio, when the configuration records a microphone.
    pub microphone: Option<AudioStream>,
}

/// Creates one bounded channel per source `configuration` requires.
pub fn media_inputs(
    configuration: &RecordingConfiguration,
    capacities: StreamCapacities,
) -> Result<(MediaSinks, MediaStreams), DataPlaneError> {
    let mut sinks = MediaSinks {
        screen: None,
        camera: None,
        microphone: None,
    };
    let mut streams = MediaStreams::default();
    if configuration.requires_source(SourceKind::Screen) {
        let (sink, stream) = video_channel(capacities.video)?;
        sinks.screen = Some(sink);
        streams.screen = Some(stream);
    }
    if configuration.requires_source(SourceKind::Camera) {
        let (sink, stream) = video_channel(capacities.video)?;
        sinks.camera = Some(sink);
        streams.camera = Some(stream);
    }
    if configuration.requires_source(SourceKind::Microphone) {
        let (sink, stream) = audio_channel(capacities.audio)?;
        sinks.microphone = Some(sink);
        streams.microphone = Some(stream);
    }
    Ok((sinks, streams))
}

#[cfg(test)]
#[path = "data_plane_tests.rs"]
mod tests;
