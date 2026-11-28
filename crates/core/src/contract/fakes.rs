//! In-memory fakes for every outbound port of the Recording application.
//!
//! The application tests in `framelet-core` and the shared contract suites
//! use the same fakes, and the fakes pass the same suites as real adapters
//! (`contract::tests`). Every fake appends the port calls it receives to a
//! shared [`Log`] and fails a call whose name was added to [`Faults`].
//!
//! Call names are `<port>.<method>`, for example `storage.publish`,
//! `media.close`, or `microphone.stop`. Two further fault names change
//! behaviour without failing a call: `media.prepare.mismatch` makes the media
//! fake return sinks for a different configuration, and `media.close.foreign`
//! makes it return a [`ClosedOutput`] for a target it was not prepared with.
//! The settings fake fails `settings.save` with
//! [`SettingsError::WriteFailed`] instead of a [`PortError`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use framelet_capture_api::{
    AudioBlock, PixelFormat, SourceDescriptor, SourceId, SourceKind, VideoFrame, VideoFrameMetadata,
};

use super::capture::CaptureSubject;
use super::media::MediaSubject;
use super::settings::SettingsSubject;
use super::storage::StorageSubject;
use crate::data_plane::{
    AudioSink, MediaStreams, Received, StreamCapacities, VideoSink, media_inputs,
};
use crate::domain::{RecordingArtifact, RecordingConfiguration, RecordingId, RecordingMode};
use crate::ports::RecordingSettingsPort;
use crate::ports::{
    CameraCapturePort, CaptureSessionId, ClosedOutput, MediaPipelinePort, MediaSessionId,
    MicrophoneCapturePort, OutputTarget, PortError, PreparedMedia, RecordingEvent,
    RecordingEventPublisher, RecordingStoragePort, ScreenCapturePort, SelectedSource,
    StorageReservation, StorageSessionId,
};
use crate::settings::{LoadedSettings, RecordingSettings, SettingsError, SettingsIoFailure};

/// Locks a fake's state. A poisoned lock only follows a panicking test;
/// recover the data so later assertions still report meaningful results.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Shared, ordered log of port calls and published events.
#[derive(Clone, Debug, Default)]
pub struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    /// Appends an entry.
    pub fn push(&self, entry: impl Into<String>) {
        lock(&self.0).push(entry.into());
    }

    /// Returns every entry in order.
    #[must_use]
    pub fn entries(&self) -> Vec<String> {
        lock(&self.0).clone()
    }

    /// Returns the port calls, without published events.
    #[must_use]
    pub fn calls(&self) -> Vec<String> {
        self.entries()
            .into_iter()
            .filter(|entry| !entry.starts_with("event:"))
            .collect()
    }

    /// Returns the published events (`event:<name>`), without port calls.
    #[must_use]
    pub fn events(&self) -> Vec<String> {
        self.entries()
            .into_iter()
            .filter(|entry| entry.starts_with("event:"))
            .collect()
    }
}

/// Names of port calls that fail with [`PortError::Failed`], plus behaviour
/// switches (see the module documentation).
#[derive(Clone, Debug, Default)]
pub struct Faults(Arc<Mutex<Vec<&'static str>>>);

impl Faults {
    /// Makes every later call named `call` fail (or enables a switch).
    pub fn add(&self, call: &'static str) {
        lock(&self.0).push(call);
    }

    /// Returns whether `call` is scripted to fail.
    #[must_use]
    pub fn is_set(&self, call: &str) -> bool {
        lock(&self.0).contains(&call)
    }

    /// Logs `call` and fails it if scripted.
    pub fn check(&self, log: &Log, call: &str) -> Result<(), PortError> {
        log.push(call);
        if self.is_set(call) {
            Err(PortError::Failed(format!("injected {call}")))
        } else {
            Ok(())
        }
    }
}

/// A valid 1x1 BGRA8 frame. Built field by field because every value is a
/// constant that satisfies the [`VideoFrame::new`] invariants.
#[must_use]
pub fn synthetic_frame(sequence: u64) -> VideoFrame {
    VideoFrame {
        metadata: VideoFrameMetadata {
            width: 1,
            height: 1,
            stride: 4,
            pixel_format: PixelFormat::Bgra8,
            timestamp: Duration::from_millis(sequence),
            sequence,
        },
        data: vec![0; 4],
    }
}

/// A valid mono block of 480 silent samples (10 ms at 48 kHz). Built field by
/// field because every value satisfies the [`AudioBlock::new`] invariants.
#[must_use]
pub fn synthetic_block(sequence: u64) -> AudioBlock {
    AudioBlock {
        sample_rate: 48_000,
        channels: 1,
        timestamp: Duration::from_millis(sequence.saturating_mul(10)),
        first_sample_position: Some(sequence.saturating_mul(480)),
        samples: vec![0.0; 480],
    }
}

// ---------------------------------------------------------------- storage

#[derive(Debug, Default)]
struct StorageState {
    next: u64,
    active: HashMap<StorageSessionId, OutputTarget>,
    candidates: HashMap<OutputTarget, Vec<u8>>,
    published: HashMap<String, Vec<u8>>,
}

/// In-memory [`RecordingStoragePort`] with the filesystem adapter's semantics:
/// exclusive reservations, publication only of the matching closed output,
/// idempotent abort that never touches published output.
#[derive(Debug)]
pub struct FakeStorage {
    log: Log,
    faults: Faults,
    state: Arc<Mutex<StorageState>>,
}

impl FakeStorage {
    /// Creates a fake storage sharing `log` and `faults`.
    #[must_use]
    pub fn new(log: Log, faults: Faults) -> Self {
        Self {
            log,
            faults,
            state: Arc::default(),
        }
    }

    /// Stands in for a media writer: stores `bytes` as the candidate content
    /// of a reserved target. Writes to targets not reserved are ignored.
    pub fn write_candidate(&self, target: &OutputTarget, bytes: &[u8]) {
        if let Some(candidate) = lock(&self.state).candidates.get_mut(target) {
            candidate.clear();
            candidate.extend_from_slice(bytes);
        }
    }

    /// Returns whether a temporary candidate exists for `target`.
    #[must_use]
    pub fn candidate_exists(&self, target: &OutputTarget) -> bool {
        lock(&self.state).candidates.contains_key(target)
    }

    /// Returns the published bytes for an artifact, if it was published.
    #[must_use]
    pub fn published(&self, artifact: &RecordingArtifact) -> Option<Vec<u8>> {
        lock(&self.state)
            .published
            .get(artifact.location())
            .cloned()
    }

    fn handle(&self) -> Self {
        Self {
            log: self.log.clone(),
            faults: self.faults.clone(),
            state: Arc::clone(&self.state),
        }
    }
}

impl RecordingStoragePort for FakeStorage {
    fn begin(
        &mut self,
        recording_id: RecordingId,
        _configuration: &RecordingConfiguration,
    ) -> Result<StorageReservation, PortError> {
        self.faults.check(&self.log, "storage.begin")?;
        let mut state = lock(&self.state);
        state.next = state
            .next
            .checked_add(1)
            .ok_or_else(|| PortError::Failed("storage identity exhausted".into()))?;
        let stem = format!("recording-{}-{}", recording_id.value(), state.next);
        let session = StorageSessionId::new(format!("fake-storage-{}", state.next))?;
        let target = OutputTarget::new(PathBuf::from(format!("fake-storage/{stem}.part")))?;
        state.active.insert(session.clone(), target.clone());
        state.candidates.insert(target.clone(), Vec::new());
        Ok(StorageReservation { session, target })
    }

    fn publish(
        &mut self,
        session: &StorageSessionId,
        output: ClosedOutput,
    ) -> Result<RecordingArtifact, PortError> {
        self.faults.check(&self.log, "storage.publish")?;
        let mut state = lock(&self.state);
        let target = state
            .active
            .get(session)
            .cloned()
            .ok_or_else(|| PortError::InvalidInput("unknown storage session".into()))?;
        if output.target() != &target {
            return Err(PortError::InvalidInput(
                "closed output belongs to another target".into(),
            ));
        }
        let location = target
            .as_path()
            .with_extension("framelet")
            .to_string_lossy()
            .into_owned();
        let artifact = RecordingArtifact::new(location)
            .map_err(|error| PortError::Failed(error.to_string()))?;
        let bytes = state.candidates.remove(&target).unwrap_or_default();
        state
            .published
            .insert(artifact.location().to_owned(), bytes);
        state.active.remove(session);
        Ok(artifact)
    }

    fn abort(&mut self, session: &StorageSessionId) -> Result<(), PortError> {
        self.faults.check(&self.log, "storage.abort")?;
        let mut state = lock(&self.state);
        if let Some(target) = state.active.remove(session) {
            state.candidates.remove(&target);
        }
        Ok(())
    }
}

/// Contract-suite subject for [`FakeStorage`].
#[derive(Debug)]
pub struct FakeStorageSubject {
    storage: FakeStorage,
}

impl Default for FakeStorageSubject {
    fn default() -> Self {
        Self {
            storage: FakeStorage::new(Log::default(), Faults::default()),
        }
    }
}

impl StorageSubject for FakeStorageSubject {
    fn storage(&mut self) -> &mut dyn RecordingStoragePort {
        &mut self.storage
    }

    fn write_media(&mut self, target: &OutputTarget, bytes: &[u8]) {
        self.storage.write_candidate(target, bytes);
    }

    fn candidate_exists(&self, target: &OutputTarget) -> bool {
        self.storage.candidate_exists(target)
    }

    fn published_media(&self, artifact: &RecordingArtifact) -> Option<Vec<u8>> {
        self.storage.published(artifact)
    }
}

impl FakeStorageSubject {
    /// Returns a second handle on the same fake state (for inspection).
    #[must_use]
    pub fn inspect(&self) -> FakeStorage {
        self.storage.handle()
    }
}

// ------------------------------------------------------------------ media

/// What the media fake consumed while draining, per prepared session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrainedMedia {
    /// Video frames taken from all video streams.
    pub video_frames: u64,
    /// Audio packets taken from the audio stream.
    pub audio_packets: u64,
    /// Audio packets that carried an explicit gap marker.
    pub marked_gaps: u64,
    /// Audio blocks lost after the last accepted packet, read from the final
    /// stream stats (trailing loss no packet carries).
    pub trailing_lost_blocks: u64,
}

#[derive(Debug)]
struct MediaSession {
    target: OutputTarget,
    streams: MediaStreams,
}

/// In-memory [`MediaPipelinePort`]. It never consumes before `drain`, so
/// sinks fill up and apply the overload policy deterministically.
#[derive(Debug)]
pub struct FakeMedia {
    log: Log,
    faults: Faults,
    capacities: StreamCapacities,
    next: u64,
    sessions: HashMap<MediaSessionId, MediaSession>,
    drained: Arc<Mutex<DrainedMedia>>,
}

/// Capacities the media fake uses unless told otherwise.
pub const FAKE_CAPACITIES: StreamCapacities = StreamCapacities { video: 4, audio: 8 };

impl FakeMedia {
    /// Creates a media fake with [`FAKE_CAPACITIES`].
    #[must_use]
    pub fn new(log: Log, faults: Faults) -> Self {
        Self {
            log,
            faults,
            capacities: FAKE_CAPACITIES,
            next: 0,
            sessions: HashMap::new(),
            drained: Arc::default(),
        }
    }

    /// Returns a shared view of what all drains have consumed so far.
    #[must_use]
    pub fn drained(&self) -> Arc<Mutex<DrainedMedia>> {
        Arc::clone(&self.drained)
    }

    fn known(&self, session: &MediaSessionId) -> Result<(), PortError> {
        if self.sessions.contains_key(session) {
            Ok(())
        } else {
            Err(PortError::InvalidInput("unknown media session".into()))
        }
    }
}

/// Returns the drained totals, recovering from a poisoned lock.
#[must_use]
pub fn drained_totals(drained: &Mutex<DrainedMedia>) -> DrainedMedia {
    *lock(drained)
}

fn mismatched(configuration: &RecordingConfiguration) -> Result<RecordingConfiguration, PortError> {
    let (mode, microphone) = match configuration.mode() {
        RecordingMode::Microphone => (RecordingMode::Screen, false),
        mode => (mode, !configuration.includes_microphone()),
    };
    RecordingConfiguration::new(mode, microphone)
        .map_err(|error| PortError::Failed(error.to_string()))
}

impl MediaPipelinePort for FakeMedia {
    fn prepare(
        &mut self,
        _recording_id: RecordingId,
        configuration: &RecordingConfiguration,
        target: &OutputTarget,
    ) -> Result<PreparedMedia, PortError> {
        self.faults.check(&self.log, "media.prepare")?;
        let configuration = if self.faults.is_set("media.prepare.mismatch") {
            mismatched(configuration)?
        } else {
            configuration.clone()
        };
        let (sinks, streams) = media_inputs(&configuration, self.capacities)?;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| PortError::Failed("media identity exhausted".into()))?;
        let session = MediaSessionId::new(format!("fake-media-{}", self.next))?;
        self.sessions.insert(
            session.clone(),
            MediaSession {
                target: target.clone(),
                streams,
            },
        );
        Ok(PreparedMedia { session, sinks })
    }

    fn start(&mut self, session: &MediaSessionId) -> Result<(), PortError> {
        self.faults.check(&self.log, "media.start")?;
        self.known(session)
    }

    fn pause(&mut self, session: &MediaSessionId) -> Result<(), PortError> {
        self.faults.check(&self.log, "media.pause")?;
        self.known(session)
    }

    fn resume(&mut self, session: &MediaSessionId) -> Result<(), PortError> {
        self.faults.check(&self.log, "media.resume")?;
        self.known(session)
    }

    fn drain(&mut self, session: &MediaSessionId) -> Result<(), PortError> {
        self.faults.check(&self.log, "media.drain")?;
        let media = self
            .sessions
            .get_mut(session)
            .ok_or_else(|| PortError::InvalidInput("unknown media session".into()))?;
        let mut drained = lock(&self.drained);
        for stream in [&mut media.streams.screen, &mut media.streams.camera]
            .into_iter()
            .flatten()
        {
            while let Received::Item(_) = stream.try_recv() {
                drained.video_frames = drained.video_frames.saturating_add(1);
            }
        }
        if let Some(stream) = &mut media.streams.microphone {
            while let Received::Item(packet) = stream.try_recv() {
                drained.audio_packets = drained.audio_packets.saturating_add(1);
                if packet.gap_before.is_some() {
                    drained.marked_gaps = drained.marked_gaps.saturating_add(1);
                }
            }
            // Consumer obligation (ADR-0020): trailing loss is only in the stats.
            if let Some(gap) = stream.stats().pending_gap {
                drained.trailing_lost_blocks =
                    drained.trailing_lost_blocks.saturating_add(gap.lost_blocks);
            }
        }
        Ok(())
    }

    fn close(&mut self, session: &MediaSessionId) -> Result<ClosedOutput, PortError> {
        let fault = self.faults.check(&self.log, "media.close");
        // The session is released whether or not finalization succeeds.
        let media = self
            .sessions
            .remove(session)
            .ok_or_else(|| PortError::InvalidInput("unknown media session".into()))?;
        fault?;
        if self.faults.is_set("media.close.foreign") {
            return Ok(ClosedOutput::new(OutputTarget::new(
                "fake-media/foreign-output.part",
            )?));
        }
        Ok(ClosedOutput::new(media.target))
    }

    fn abort(&mut self, session: &MediaSessionId) -> Result<(), PortError> {
        self.faults.check(&self.log, "media.abort")?;
        self.sessions.remove(session);
        Ok(())
    }
}

/// Contract-suite subject for [`FakeMedia`].
#[derive(Debug)]
pub struct FakeMediaSubject {
    media: FakeMedia,
    next_target: u64,
}

impl Default for FakeMediaSubject {
    fn default() -> Self {
        Self {
            media: FakeMedia::new(Log::default(), Faults::default()),
            next_target: 0,
        }
    }
}

impl MediaSubject for FakeMediaSubject {
    fn media(&mut self) -> &mut dyn MediaPipelinePort {
        &mut self.media
    }

    fn target(&mut self) -> OutputTarget {
        self.next_target = self.next_target.saturating_add(1);
        match OutputTarget::new(format!("fake-media/target-{}.part", self.next_target)) {
            Ok(target) => target,
            Err(error) => panic!("fake target must be valid: {error}"),
        }
    }
}

// ---------------------------------------------------------------- capture

/// A sink the capture fake can feed with synthetic media.
pub trait FakeFeed: Send + 'static {
    /// Offers one synthetic item; the delivery outcome is visible on the monitor.
    fn feed(&mut self, sequence: u64);
    /// Reports source loss through the sink.
    fn lose(self);
}

impl FakeFeed for VideoSink {
    fn feed(&mut self, sequence: u64) {
        let _delivery = self.try_send(synthetic_frame(sequence));
    }

    fn lose(self) {
        self.report_source_lost();
    }
}

impl FakeFeed for AudioSink {
    fn feed(&mut self, sequence: u64) {
        let _delivery = self.try_send(synthetic_block(sequence));
    }

    fn lose(self) {
        self.report_source_lost();
    }
}

#[derive(Debug)]
struct CaptureState<S> {
    next: u64,
    produced: u64,
    active: Vec<(CaptureSessionId, S)>,
}

/// In-memory capture port for one source kind. `S` is [`VideoSink`] for the
/// screen and camera fakes and [`AudioSink`] for the microphone fake.
///
/// Every source is available except ids starting with `missing`, which
/// fail to start with [`PortError::SourceUnavailable`].
#[derive(Debug)]
pub struct FakeCapture<S> {
    prefix: &'static str,
    kind: SourceKind,
    log: Log,
    faults: Faults,
    state: Arc<Mutex<CaptureState<S>>>,
}

/// Test-side control of a [`FakeCapture`]: produce media or lose the source.
#[derive(Debug)]
pub struct FakeCaptureControl<S> {
    state: Arc<Mutex<CaptureState<S>>>,
}

impl<S> Clone for FakeCaptureControl<S> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<S: FakeFeed> FakeCaptureControl<S> {
    /// Offers `count` synthetic items to the sink of `session`, if active.
    pub fn produce(&self, session: &CaptureSessionId, count: usize) {
        let mut state = lock(&self.state);
        let CaptureState {
            produced, active, ..
        } = &mut *state;
        if let Some((_, sink)) = active.iter_mut().find(|(id, _)| id == session) {
            for _ in 0..count {
                sink.feed(*produced);
                *produced = produced.saturating_add(1);
            }
        }
    }

    /// Loses the source of `session`: its sink reports the loss and is gone.
    pub fn lose(&self, session: &CaptureSessionId) {
        let mut state = lock(&self.state);
        if let Some(position) = state.active.iter().position(|(id, _)| id == session) {
            let (_, sink) = state.active.remove(position);
            sink.lose();
        }
    }

    /// Loses the source of every active session.
    pub fn lose_all(&self) {
        let active = std::mem::take(&mut lock(&self.state).active);
        for (_, sink) in active {
            sink.lose();
        }
    }

    /// Returns the active capture sessions.
    #[must_use]
    pub fn active(&self) -> Vec<CaptureSessionId> {
        lock(&self.state)
            .active
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl<S: FakeFeed> FakeCapture<S> {
    fn with(prefix: &'static str, kind: SourceKind, log: Log, faults: Faults) -> Self {
        Self {
            prefix,
            kind,
            log,
            faults,
            state: Arc::new(Mutex::new(CaptureState {
                next: 0,
                produced: 0,
                active: Vec::new(),
            })),
        }
    }

    /// Returns a control handle sharing this fake's state.
    #[must_use]
    pub fn control(&self) -> FakeCaptureControl<S> {
        FakeCaptureControl {
            state: Arc::clone(&self.state),
        }
    }

    fn sources(&self) -> Result<Vec<SourceDescriptor>, PortError> {
        let id = SourceId::new(format!("{}-source", self.prefix))
            .map_err(|_| PortError::Failed("invalid fake source id".into()))?;
        Ok(vec![SourceDescriptor {
            id,
            name: format!("Fake {}", self.prefix),
            kind: self.kind,
        }])
    }

    fn begin(&mut self, source: &SelectedSource, sink: S) -> Result<CaptureSessionId, PortError> {
        // On every error path `sink` is dropped here, ending its stream.
        self.faults
            .check(&self.log, &format!("{}.start", self.prefix))?;
        if source.id.as_str().starts_with("missing") {
            return Err(PortError::SourceUnavailable);
        }
        let mut state = lock(&self.state);
        state.next = state
            .next
            .checked_add(1)
            .ok_or_else(|| PortError::Failed("capture identity exhausted".into()))?;
        let session = CaptureSessionId::new(format!("{}-{}", self.prefix, state.next))?;
        state.active.push((session.clone(), sink));
        Ok(session)
    }

    fn end(&mut self, session: &CaptureSessionId) -> Result<(), PortError> {
        self.faults
            .check(&self.log, &format!("{}.stop", self.prefix))?;
        let released = {
            let mut state = lock(&self.state);
            state
                .active
                .iter()
                .position(|(id, _)| id == session)
                .map(|position| state.active.remove(position))
        };
        // Dropping the sink ends its stream; stopping twice is a no-op.
        drop(released);
        Ok(())
    }
}

impl FakeCapture<VideoSink> {
    /// Creates a screen capture fake (calls logged as `screen.*`).
    #[must_use]
    pub fn screen(log: Log, faults: Faults) -> Self {
        Self::with("screen", SourceKind::Screen, log, faults)
    }

    /// Creates a camera capture fake (calls logged as `camera.*`).
    #[must_use]
    pub fn camera(log: Log, faults: Faults) -> Self {
        Self::with("camera", SourceKind::Camera, log, faults)
    }
}

impl FakeCapture<AudioSink> {
    /// Creates a microphone capture fake (calls logged as `microphone.*`).
    #[must_use]
    pub fn microphone(log: Log, faults: Faults) -> Self {
        Self::with("microphone", SourceKind::Microphone, log, faults)
    }
}

impl ScreenCapturePort for FakeCapture<VideoSink> {
    fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError> {
        self.sources()
    }
    fn start(
        &mut self,
        source: &SelectedSource,
        sink: VideoSink,
    ) -> Result<CaptureSessionId, PortError> {
        self.begin(source, sink)
    }
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError> {
        self.end(session)
    }
}

impl CameraCapturePort for FakeCapture<VideoSink> {
    fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError> {
        self.sources()
    }
    fn start(
        &mut self,
        source: &SelectedSource,
        sink: VideoSink,
    ) -> Result<CaptureSessionId, PortError> {
        self.begin(source, sink)
    }
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError> {
        self.end(session)
    }
}

impl MicrophoneCapturePort for FakeCapture<AudioSink> {
    fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError> {
        self.sources()
    }
    fn start(
        &mut self,
        source: &SelectedSource,
        sink: AudioSink,
    ) -> Result<CaptureSessionId, PortError> {
        self.begin(source, sink)
    }
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError> {
        self.end(session)
    }
}

/// Contract-suite subject for a [`FakeCapture`].
#[derive(Debug)]
pub struct FakeCaptureSubject<S> {
    capture: FakeCapture<S>,
    control: FakeCaptureControl<S>,
}

impl FakeCaptureSubject<VideoSink> {
    /// Subject for the screen capture fake.
    #[must_use]
    pub fn screen() -> Self {
        Self::of(FakeCapture::screen(Log::default(), Faults::default()))
    }

    /// Subject for the camera capture fake.
    #[must_use]
    pub fn camera() -> Self {
        Self::of(FakeCapture::camera(Log::default(), Faults::default()))
    }
}

impl FakeCaptureSubject<AudioSink> {
    /// Subject for the microphone capture fake.
    #[must_use]
    pub fn microphone() -> Self {
        Self::of(FakeCapture::microphone(Log::default(), Faults::default()))
    }
}

impl<S: FakeFeed> FakeCaptureSubject<S> {
    fn of(capture: FakeCapture<S>) -> Self {
        let control = capture.control();
        Self { capture, control }
    }

    fn selected(&self, id: &str) -> SelectedSource {
        match SourceId::new(id) {
            Ok(id) => SelectedSource {
                display_name: id.as_str().to_owned(),
                id,
            },
            Err(error) => panic!("fake source id must be valid: {error:?}"),
        }
    }
}

impl<S> CaptureSubject for FakeCaptureSubject<S>
where
    S: FakeFeed + super::capture::ContractSink,
{
    type Sink = S;

    fn source(&mut self) -> SelectedSource {
        self.selected(&format!("{}-source", self.capture.prefix))
    }

    fn missing_source(&mut self) -> SelectedSource {
        self.selected("missing-source")
    }

    fn start(
        &mut self,
        source: &SelectedSource,
        sink: Self::Sink,
    ) -> Result<CaptureSessionId, PortError> {
        self.capture.begin(source, sink)
    }

    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError> {
        self.capture.end(session)
    }

    fn produce(&mut self, session: &CaptureSessionId, count: usize) {
        self.control.produce(session, count);
    }

    fn lose_source(&mut self, session: &CaptureSessionId) {
        self.control.lose(session);
    }
}

// ----------------------------------------------------------------- events

/// Event publisher that logs `event:<name>` and keeps every event.
#[derive(Clone, Debug, Default)]
pub struct FakeEvents {
    log: Log,
    history: Arc<Mutex<Vec<RecordingEvent>>>,
}

impl FakeEvents {
    /// Creates a publisher logging into `log`.
    #[must_use]
    pub fn new(log: Log) -> Self {
        Self {
            log,
            history: Arc::default(),
        }
    }

    /// Returns every published event in order.
    #[must_use]
    pub fn history(&self) -> Vec<RecordingEvent> {
        lock(&self.history).clone()
    }
}

impl RecordingEventPublisher for FakeEvents {
    fn publish(&self, event: RecordingEvent) {
        let name = match &event {
            RecordingEvent::Ready { .. } => "ready",
            RecordingEvent::Started { .. } => "started",
            RecordingEvent::Paused { .. } => "paused",
            RecordingEvent::Resumed { .. } => "resumed",
            RecordingEvent::Completed { .. } => "completed",
            RecordingEvent::Failed { .. } => "failed",
            RecordingEvent::Cancelled { .. } => "cancelled",
            RecordingEvent::SourceLost { .. } => "source_lost",
        };
        self.log.push(format!("event:{name}"));
        lock(&self.history).push(event);
    }
}

// --------------------------------------------------------------- settings

/// What the settings fake holds.
#[derive(Clone, Debug, Eq, PartialEq)]
enum FakeStoredSettings {
    /// Settings a previous `save` stored.
    Valid(RecordingSettings),
    /// Content the adapter cannot accept, as a hand-edited broken file.
    Unusable,
}

/// In-memory [`RecordingSettingsPort`] with the TOML adapter's semantics:
/// defaults when nothing is stored, defaults plus a typed error for unusable
/// content (left unchanged), and all-or-nothing saves.
#[derive(Clone, Debug)]
pub struct FakeSettings {
    log: Log,
    faults: Faults,
    stored: Arc<Mutex<Option<FakeStoredSettings>>>,
}

impl FakeSettings {
    /// Creates an empty store logging into `log` and failing per `faults`.
    #[must_use]
    pub fn new(log: Log, faults: Faults) -> Self {
        Self {
            log,
            faults,
            stored: Arc::default(),
        }
    }

    /// Replaces the stored content with content `load` must reject.
    pub fn store_unusable(&self) {
        *lock(&self.stored) = Some(FakeStoredSettings::Unusable);
    }

    /// Returns a stable representation of the stored content, if any.
    #[must_use]
    pub fn stored_bytes(&self) -> Option<Vec<u8>> {
        lock(&self.stored)
            .as_ref()
            .map(|stored| format!("{stored:?}").into_bytes())
    }
}

impl RecordingSettingsPort for FakeSettings {
    fn load(&self) -> LoadedSettings {
        self.log.push("settings.load");
        match lock(&self.stored).clone() {
            None => LoadedSettings::defaults_not_stored(),
            Some(FakeStoredSettings::Valid(settings)) => LoadedSettings::stored(settings),
            Some(FakeStoredSettings::Unusable) => {
                LoadedSettings::defaults_after_error(SettingsError::Malformed { line: None })
            }
        }
    }

    fn save(&self, settings: &RecordingSettings) -> Result<(), SettingsError> {
        self.log.push("settings.save");
        if self.faults.is_set("settings.save") {
            return Err(SettingsError::WriteFailed(SettingsIoFailure::Other));
        }
        *lock(&self.stored) = Some(FakeStoredSettings::Valid(settings.clone()));
        Ok(())
    }
}

/// Contract-suite subject for [`FakeSettings`].
#[derive(Debug)]
pub struct FakeSettingsSubject {
    settings: FakeSettings,
    faults: Faults,
}

impl Default for FakeSettingsSubject {
    fn default() -> Self {
        let faults = Faults::default();
        Self {
            settings: FakeSettings::new(Log::default(), faults.clone()),
            faults,
        }
    }
}

impl SettingsSubject for FakeSettingsSubject {
    fn settings(&self) -> &dyn RecordingSettingsPort {
        &self.settings
    }

    fn stored_bytes(&self) -> Option<Vec<u8>> {
        self.settings.stored_bytes()
    }

    fn store_unusable_settings(&mut self) {
        self.settings.store_unusable();
    }

    fn make_saves_fail(&mut self) -> bool {
        self.faults.add("settings.save");
        true
    }
}
