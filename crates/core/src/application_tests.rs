//! Application-service tests with fake ports; no devices or native runtimes.

use std::sync::{Arc, Mutex};

use framelet_capture_api::{AudioBlock, SourceDescriptor, SourceId, VideoFrame};

use super::*;
use crate::domain::{RecordingArtifact, RecordingMode};

/// Shared, ordered log of port calls and published events.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    fn push(&self, entry: impl Into<String>) {
        self.0.lock().unwrap().push(entry.into());
    }
    fn entries(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
    fn calls(&self) -> Vec<String> {
        self.entries()
            .into_iter()
            .filter(|e| !e.starts_with("event:"))
            .collect()
    }
    fn events(&self) -> Vec<String> {
        self.entries()
            .into_iter()
            .filter(|e| e.starts_with("event:"))
            .collect()
    }
}

/// Names of port calls that should fail with `PortError::Failed`.
#[derive(Clone, Default)]
struct Faults(Arc<Mutex<Vec<&'static str>>>);

impl Faults {
    fn add(&self, call: &'static str) {
        self.0.lock().unwrap().push(call);
    }
    fn check(&self, log: &Log, call: &'static str) -> Result<(), PortError> {
        log.push(call);
        if self.0.lock().unwrap().contains(&call) {
            Err(PortError::Failed(format!("injected {call}")))
        } else {
            Ok(())
        }
    }
}

struct FakeStorage(Log, Faults);
impl RecordingStoragePort for FakeStorage {
    fn begin(
        &mut self,
        _: RecordingId,
        _: &RecordingConfiguration,
    ) -> Result<MediaSessionId, PortError> {
        self.1.check(&self.0, "storage.begin")?;
        MediaSessionId::new("storage-1")
    }
    fn publish(&mut self, _: &MediaSessionId) -> Result<RecordingArtifact, PortError> {
        self.1.check(&self.0, "storage.publish")?;
        RecordingArtifact::new("recordings/session.mkv".into())
            .map_err(|e| PortError::Failed(e.to_string()))
    }
    fn abort(&mut self, _: &MediaSessionId) -> Result<(), PortError> {
        self.1.check(&self.0, "storage.abort")
    }
}

struct FakeMedia(Log, Faults);
impl MediaPipelinePort for FakeMedia {
    fn prepare(&mut self, _: RecordingId, _: RecordingMode) -> Result<MediaSessionId, PortError> {
        self.1.check(&self.0, "media.prepare")?;
        MediaSessionId::new("media-1")
    }
    fn start(&mut self, _: &MediaSessionId) -> Result<(), PortError> {
        self.1.check(&self.0, "media.start")
    }
    fn pause(&mut self, _: &MediaSessionId) -> Result<(), PortError> {
        self.1.check(&self.0, "media.pause")
    }
    fn resume(&mut self, _: &MediaSessionId) -> Result<(), PortError> {
        self.1.check(&self.0, "media.resume")
    }
    fn push_video(&mut self, _: &MediaSessionId, _: VideoFrame) -> Result<(), PortError> {
        self.1.check(&self.0, "media.push_video")
    }
    fn push_audio(&mut self, _: &MediaSessionId, _: AudioBlock) -> Result<(), PortError> {
        self.1.check(&self.0, "media.push_audio")
    }
    fn drain(&mut self, _: &MediaSessionId) -> Result<(), PortError> {
        self.1.check(&self.0, "media.drain")
    }
    fn close(&mut self, _: &MediaSessionId) -> Result<(), PortError> {
        self.1.check(&self.0, "media.close")
    }
}

macro_rules! fake_capture {
    ($name:ident, $trait:ident, $prefix:literal) => {
        struct $name(Log, Faults);
        impl $trait for $name {
            fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError> {
                Ok(Vec::new())
            }
            fn start(&mut self, _: &SelectedSource) -> Result<CaptureSessionId, PortError> {
                self.1.check(&self.0, concat!($prefix, ".start"))?;
                CaptureSessionId::new(concat!($prefix, "-1"))
            }
            fn stop(&mut self, _: &CaptureSessionId) -> Result<(), PortError> {
                self.1.check(&self.0, concat!($prefix, ".stop"))
            }
        }
    };
}
fake_capture!(FakeScreen, ScreenCapturePort, "screen");
fake_capture!(FakeCamera, CameraCapturePort, "camera");
fake_capture!(FakeMicrophone, MicrophoneCapturePort, "microphone");

struct FakeEvents(Log);
impl RecordingEventPublisher for FakeEvents {
    fn publish(&self, event: RecordingEvent) {
        let name = match event {
            RecordingEvent::Ready { .. } => "ready",
            RecordingEvent::Started { .. } => "started",
            RecordingEvent::Paused { .. } => "paused",
            RecordingEvent::Resumed { .. } => "resumed",
            RecordingEvent::Completed { .. } => "completed",
            RecordingEvent::Failed { .. } => "failed",
            RecordingEvent::Cancelled { .. } => "cancelled",
            RecordingEvent::SourceLost { .. } => "source_lost",
        };
        self.0.push(format!("event:{name}"));
    }
}

struct Harness {
    app: RecordingApplication,
    log: Log,
    faults: Faults,
}

fn harness_with(camera: bool) -> Harness {
    let log = Log::default();
    let faults = Faults::default();
    let ports = RecordingPorts {
        storage: Box::new(FakeStorage(log.clone(), faults.clone())),
        media: Box::new(FakeMedia(log.clone(), faults.clone())),
        screen: Some(Box::new(FakeScreen(log.clone(), faults.clone()))),
        camera: camera.then(|| {
            Box::new(FakeCamera(log.clone(), faults.clone())) as Box<dyn CameraCapturePort>
        }),
        microphone: Some(Box::new(FakeMicrophone(log.clone(), faults.clone()))),
        events: Box::new(FakeEvents(log.clone())),
    };
    Harness {
        app: RecordingApplication::new(ports),
        log,
        faults,
    }
}

fn harness() -> Harness {
    harness_with(true)
}

fn source(name: &str) -> Option<SelectedSource> {
    Some(SelectedSource {
        id: SourceId::new(name).unwrap(),
        display_name: name.into(),
    })
}

fn request(id: u64, mode: RecordingMode, microphone: bool) -> PrepareRecording {
    let configuration = RecordingConfiguration::new(mode, microphone).unwrap();
    let sources = RecordingSources {
        screen: configuration
            .requires_source(SourceKind::Screen)
            .then(|| source("display-1"))
            .flatten(),
        camera: configuration
            .requires_source(SourceKind::Camera)
            .then(|| source("camera-1"))
            .flatten(),
        microphone: configuration
            .requires_source(SourceKind::Microphone)
            .then(|| source("mic-1"))
            .flatten(),
    };
    PrepareRecording {
        recording_id: RecordingId::new(id),
        configuration,
        sources,
    }
}

fn id(value: u64) -> RecordingId {
    RecordingId::new(value)
}

fn failed_capability(error: &ApplicationError) -> Option<RecordingCapability> {
    match error {
        ApplicationError::CapabilityFailed { failure, .. } => Some(failure.capability),
        _ => None,
    }
}

#[test]
fn full_lifecycle_orchestrates_ports_in_order_and_publishes_only_after_finalization() {
    let mut h = harness();
    assert_eq!(
        h.app
            .prepare(request(1, RecordingMode::Screen, true))
            .unwrap(),
        RecordingState::Ready
    );
    assert_eq!(h.app.start(id(1)).unwrap(), RecordingState::Recording);
    assert_eq!(h.app.pause(id(1)).unwrap(), RecordingState::Paused);
    assert_eq!(h.app.resume(id(1)).unwrap(), RecordingState::Recording);
    assert!(h.app.session(id(1)).unwrap().artifact().is_none());
    assert_eq!(h.app.stop(id(1)).unwrap(), RecordingState::Completed);

    assert_eq!(
        h.log.calls(),
        [
            "storage.begin",
            "media.prepare",
            "media.start",
            "screen.start",
            "microphone.start",
            "media.pause",
            "media.resume",
            "screen.stop",
            "microphone.stop",
            "media.drain",
            "media.close",
            "storage.publish",
        ]
    );
    assert_eq!(
        h.log.events(),
        [
            "event:ready",
            "event:started",
            "event:paused",
            "event:resumed",
            "event:completed"
        ]
    );
    assert_eq!(
        h.app.session(id(1)).unwrap().artifact().unwrap().location(),
        "recordings/session.mkv"
    );
}

#[test]
fn each_mode_starts_exactly_its_required_sources() {
    let cases: [(RecordingMode, bool, &[&str]); 5] = [
        (RecordingMode::Screen, false, &["screen.start"]),
        (RecordingMode::Microphone, false, &["microphone.start"]),
        (RecordingMode::Webcam, false, &["camera.start"]),
        (
            RecordingMode::Webcam,
            true,
            &["camera.start", "microphone.start"],
        ),
        (
            RecordingMode::ScreenAndWebcam,
            true,
            &["screen.start", "camera.start", "microphone.start"],
        ),
    ];
    for (mode, microphone, expected) in cases {
        let mut h = harness();
        h.app.prepare(request(1, mode, microphone)).unwrap();
        h.app.start(id(1)).unwrap();
        let started: Vec<String> = h
            .log
            .calls()
            .into_iter()
            .filter(|c| c.ends_with(".start") && !c.starts_with("media"))
            .collect();
        assert_eq!(started, expected, "{mode:?} microphone={microphone}");
    }
}

#[test]
fn publication_failure_fails_session_without_artifact_and_keeps_candidate_for_recovery() {
    let mut h = harness();
    h.faults.add("storage.publish");
    h.app
        .prepare(request(2, RecordingMode::Screen, false))
        .unwrap();
    h.app.start(id(2)).unwrap();
    let error = h.app.stop(id(2)).unwrap_err();
    assert_eq!(
        failed_capability(&error),
        Some(RecordingCapability::Storage)
    );
    let session = h.app.session(id(2)).unwrap();
    assert_eq!(session.state(), RecordingState::Failed);
    assert!(session.artifact().is_none());
    assert!(!h.log.calls().contains(&"storage.abort".to_string()));
    assert_eq!(h.log.events().last().unwrap(), "event:failed");
}

#[test]
fn drain_failure_still_closes_pipeline_and_never_publishes() {
    let mut h = harness();
    h.faults.add("media.drain");
    h.app
        .prepare(request(3, RecordingMode::Microphone, false))
        .unwrap();
    h.app.start(id(3)).unwrap();
    let error = h.app.stop(id(3)).unwrap_err();
    assert_eq!(
        failed_capability(&error),
        Some(RecordingCapability::MediaPipeline)
    );
    let calls = h.log.calls();
    assert!(calls.contains(&"media.close".to_string()));
    assert!(!calls.contains(&"storage.publish".to_string()));
    assert_eq!(h.app.state(id(3)).unwrap(), RecordingState::Failed);
}

#[test]
fn capture_stop_failure_is_reported_and_prevents_success() {
    let mut h = harness();
    h.faults.add("screen.stop");
    h.app
        .prepare(request(4, RecordingMode::Screen, true))
        .unwrap();
    h.app.start(id(4)).unwrap();
    let error = h.app.stop(id(4)).unwrap_err();
    assert_eq!(
        failed_capability(&error),
        Some(RecordingCapability::ScreenCapture)
    );
    // The remaining source is still stopped and the pipeline still drained/closed.
    let calls = h.log.calls();
    for expected in ["microphone.stop", "media.drain", "media.close"] {
        assert!(calls.contains(&expected.to_string()), "{expected} missing");
    }
    assert!(!calls.contains(&"storage.publish".to_string()));
}

#[test]
fn capture_start_failure_rolls_back_started_sources_and_reservations() {
    let mut h = harness();
    h.faults.add("microphone.start");
    h.app
        .prepare(request(5, RecordingMode::Screen, true))
        .unwrap();
    let error = h.app.start(id(5)).unwrap_err();
    assert_eq!(
        failed_capability(&error),
        Some(RecordingCapability::MicrophoneCapture)
    );
    let calls = h.log.calls();
    let tail: Vec<&str> = calls
        .iter()
        .skip_while(|c| *c != "microphone.start")
        .map(String::as_str)
        .collect();
    assert_eq!(
        tail,
        [
            "microphone.start",
            "screen.stop",
            "media.close",
            "storage.abort"
        ]
    );
    assert_eq!(h.app.state(id(5)).unwrap(), RecordingState::Failed);
}

#[test]
fn rollback_failures_are_reported_not_swallowed() {
    let mut h = harness();
    h.faults.add("media.start");
    h.faults.add("storage.abort");
    h.app
        .prepare(request(6, RecordingMode::Screen, false))
        .unwrap();
    match h.app.start(id(6)).unwrap_err() {
        ApplicationError::CapabilityFailed {
            failure,
            additional_failures,
        } => {
            assert_eq!(failure.capability, RecordingCapability::MediaPipeline);
            assert_eq!(additional_failures.len(), 1);
            assert_eq!(
                additional_failures[0].capability,
                RecordingCapability::Storage
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn pipeline_prepare_failure_aborts_storage_and_creates_no_session() {
    let mut h = harness();
    h.faults.add("media.prepare");
    let error = h
        .app
        .prepare(request(7, RecordingMode::Screen, false))
        .unwrap_err();
    assert_eq!(
        failed_capability(&error),
        Some(RecordingCapability::MediaPipeline)
    );
    assert_eq!(
        h.log.calls(),
        ["storage.begin", "media.prepare", "storage.abort"]
    );
    assert_eq!(
        h.app.state(id(7)),
        Err(ApplicationError::RecordingNotFound(id(7)))
    );
}

#[test]
fn source_selection_must_match_configuration_before_any_port_is_called() {
    let mut h = harness();
    let mut missing = request(8, RecordingMode::Screen, true);
    missing.sources.microphone = None;
    assert_eq!(
        h.app.prepare(missing),
        Err(ApplicationError::Domain(DomainError::MissingSource(
            SourceKind::Microphone
        )))
    );
    let mut unexpected = request(9, RecordingMode::Microphone, false);
    unexpected.sources.camera = source("camera-1");
    assert_eq!(
        h.app.prepare(unexpected),
        Err(ApplicationError::Domain(DomainError::UnexpectedSource(
            SourceKind::Camera
        )))
    );
    assert!(h.log.entries().is_empty());
}

#[test]
fn unavailable_capability_is_rejected_before_any_port_is_called() {
    let mut h = harness_with(false);
    assert_eq!(
        h.app.prepare(request(10, RecordingMode::Webcam, false)),
        Err(ApplicationError::CapabilityUnavailable(
            RecordingCapability::CameraCapture
        ))
    );
    assert!(h.log.entries().is_empty());
    // Modes that do not need the camera still work.
    assert_eq!(
        h.app
            .prepare(request(11, RecordingMode::Screen, false))
            .unwrap(),
        RecordingState::Ready
    );
}

#[test]
fn cancel_releases_prepared_resources_and_blocks_start() {
    let mut h = harness();
    h.app
        .prepare(request(12, RecordingMode::Screen, false))
        .unwrap();
    assert_eq!(h.app.cancel(id(12)).unwrap(), RecordingState::Cancelled);
    assert_eq!(
        h.log.calls(),
        [
            "storage.begin",
            "media.prepare",
            "media.close",
            "storage.abort"
        ]
    );
    assert_eq!(h.log.events(), ["event:ready", "event:cancelled"]);
    assert!(h.app.start(id(12)).is_err());
}

#[test]
fn cancel_with_cleanup_failure_cancels_but_reports_failure() {
    let mut h = harness();
    h.faults.add("storage.abort");
    h.app
        .prepare(request(13, RecordingMode::Screen, false))
        .unwrap();
    match h.app.cancel(id(13)).unwrap_err() {
        ApplicationError::CleanupIncomplete { state, failures } => {
            assert_eq!(state, RecordingState::Cancelled);
            assert_eq!(failures[0].capability, RecordingCapability::Storage);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(h.app.state(id(13)).unwrap(), RecordingState::Cancelled);
}

#[test]
fn illegal_commands_never_reach_a_port() {
    let mut h = harness();
    h.app
        .prepare(request(14, RecordingMode::Screen, false))
        .unwrap();
    let before = h.log.entries();
    assert!(h.app.pause(id(14)).is_err());
    assert!(h.app.resume(id(14)).is_err());
    assert!(h.app.stop(id(14)).is_err());
    assert_eq!(h.log.entries(), before);
    assert_eq!(h.app.state(id(14)).unwrap(), RecordingState::Ready);

    h.app.start(id(14)).unwrap();
    let before = h.log.entries();
    assert!(h.app.cancel(id(14)).is_err());
    assert!(h.app.start(id(14)).is_err());
    assert_eq!(h.log.entries(), before);
}

#[test]
fn pause_failure_keeps_recording_state() {
    let mut h = harness();
    h.faults.add("media.pause");
    h.app
        .prepare(request(15, RecordingMode::Screen, false))
        .unwrap();
    h.app.start(id(15)).unwrap();
    assert!(h.app.pause(id(15)).is_err());
    assert_eq!(h.app.state(id(15)).unwrap(), RecordingState::Recording);
    assert!(!h.log.events().contains(&"event:paused".to_string()));
}

#[test]
fn duplicate_session_id_is_rejected() {
    let mut h = harness();
    h.app
        .prepare(request(16, RecordingMode::Screen, false))
        .unwrap();
    assert_eq!(
        h.app.prepare(request(16, RecordingMode::Screen, false)),
        Err(ApplicationError::RecordingAlreadyExists(id(16)))
    );
}

#[test]
fn unknown_session_is_reported() {
    let mut h = harness();
    assert_eq!(
        h.app.start(id(99)),
        Err(ApplicationError::RecordingNotFound(id(99)))
    );
}
