//! Application-service tests with the shared fakes from [`crate::contract::fakes`];
//! no devices or native runtimes.

use std::sync::{Arc, Mutex};

use framelet_capture_api::SourceId;

use super::*;
use crate::contract::fakes::{
    DrainedMedia, FakeCapture, FakeCaptureControl, FakeEvents, FakeMedia, FakeStorage, Faults, Log,
    drained_totals,
};
use crate::data_plane::{AudioSink, VideoSink};
use crate::domain::RecordingMode;

struct Harness {
    app: RecordingApplication,
    log: Log,
    faults: Faults,
    events: FakeEvents,
    screen: FakeCaptureControl<VideoSink>,
    camera: FakeCaptureControl<VideoSink>,
    microphone: FakeCaptureControl<AudioSink>,
    drained: Arc<Mutex<DrainedMedia>>,
}

impl Harness {
    fn drained(&self) -> DrainedMedia {
        drained_totals(&self.drained)
    }
}

fn harness_with(camera: bool) -> Harness {
    let log = Log::default();
    let faults = Faults::default();
    let screen = FakeCapture::screen(log.clone(), faults.clone());
    let camera_fake = FakeCapture::camera(log.clone(), faults.clone());
    let microphone = FakeCapture::microphone(log.clone(), faults.clone());
    let media = FakeMedia::new(log.clone(), faults.clone());
    let events = FakeEvents::new(log.clone());
    let controls = (
        screen.control(),
        camera_fake.control(),
        microphone.control(),
    );
    let drained = media.drained();
    let ports = RecordingPorts {
        storage: Box::new(FakeStorage::new(log.clone(), faults.clone())),
        media: Box::new(media),
        screen: Some(Box::new(screen)),
        camera: camera.then(|| Box::new(camera_fake) as Box<dyn CameraCapturePort>),
        microphone: Some(Box::new(microphone)),
        events: Box::new(events.clone()),
    };
    Harness {
        app: RecordingApplication::new(ports),
        log,
        faults,
        events,
        screen: controls.0,
        camera: controls.1,
        microphone: controls.2,
        drained,
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
        "fake-storage/recording-1-1.framelet"
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
            "media.abort",
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
            "media.abort",
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

fn source_lost_events(h: &Harness) -> Vec<(SourceKind, String)> {
    h.events
        .history()
        .into_iter()
        .filter_map(|event| match event {
            RecordingEvent::SourceLost { kind, source, .. } => {
                Some((kind, source.as_str().to_owned()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn stop_is_idempotent_and_touches_no_port_after_the_session_ended() {
    let mut h = harness();
    h.app
        .prepare(request(20, RecordingMode::Screen, false))
        .unwrap();
    h.app.start(id(20)).unwrap();
    assert_eq!(h.app.stop(id(20)).unwrap(), RecordingState::Completed);
    let before = h.log.entries();
    assert_eq!(h.app.stop(id(20)).unwrap(), RecordingState::Completed);
    assert_eq!(h.app.stop(id(20)).unwrap(), RecordingState::Completed);
    assert_eq!(
        h.log.entries(),
        before,
        "a repeated stop must touch no port"
    );

    // A stop that failed is equally final: repeating it neither retries nor publishes.
    h.faults.add("media.drain");
    h.app
        .prepare(request(21, RecordingMode::Screen, false))
        .unwrap();
    h.app.start(id(21)).unwrap();
    assert!(h.app.stop(id(21)).is_err());
    let before = h.log.entries();
    assert_eq!(h.app.stop(id(21)).unwrap(), RecordingState::Failed);
    assert_eq!(h.log.entries(), before);

    // Cancelled sessions were never started; stop reports their final state.
    h.app
        .prepare(request(22, RecordingMode::Screen, false))
        .unwrap();
    h.app.cancel(id(22)).unwrap();
    assert_eq!(h.app.stop(id(22)).unwrap(), RecordingState::Cancelled);
}

#[test]
fn captured_media_flows_through_core_owned_sinks_to_the_pipeline() {
    let mut h = harness();
    h.app
        .prepare(request(23, RecordingMode::ScreenAndWebcam, true))
        .unwrap();
    h.app.start(id(23)).unwrap();
    for session in h.screen.active() {
        h.screen.produce(&session, 2);
    }
    for session in h.camera.active() {
        h.camera.produce(&session, 3);
    }
    for session in h.microphone.active() {
        h.microphone.produce(&session, 5);
    }
    assert_eq!(h.app.stop(id(23)).unwrap(), RecordingState::Completed);
    assert_eq!(
        h.drained(),
        DrainedMedia {
            video_frames: 5,
            audio_packets: 5,
            marked_gaps: 0,
            trailing_lost_blocks: 0
        }
    );
    assert!(h.screen.active().is_empty(), "stop must release every sink");
}

#[test]
fn trailing_audio_loss_reaches_the_pipeline_through_final_stats() {
    let mut h = harness();
    h.app
        .prepare(request(34, RecordingMode::Microphone, false))
        .unwrap();
    h.app.start(id(34)).unwrap();
    let session = h.microphone.active().remove(0);
    // The fake pipeline consumes only while draining: 8 blocks fit, 4 are lost
    // after the last accepted block, so no packet can carry the gap.
    h.microphone.produce(&session, 12);
    assert_eq!(h.app.stop(id(34)).unwrap(), RecordingState::Completed);
    assert_eq!(
        h.drained(),
        DrainedMedia {
            video_frames: 0,
            audio_packets: 8,
            marked_gaps: 0,
            trailing_lost_blocks: 4
        }
    );
}

#[test]
fn essential_source_loss_stops_without_publishing_and_keeps_candidate() {
    let mut h = harness();
    h.app
        .prepare(request(25, RecordingMode::Screen, true))
        .unwrap();
    h.app.start(id(25)).unwrap();
    assert_eq!(
        h.app.process_source_losses(id(25)).unwrap(),
        RecordingState::Recording,
        "nothing lost yet"
    );
    h.screen.lose_all();
    let error = h.app.process_source_losses(id(25)).unwrap_err();
    assert!(
        matches!(
            error,
            ApplicationError::SourceLost {
                kind: SourceKind::Screen,
                ..
            }
        ),
        "{error:?}"
    );
    let session = h.app.session(id(25)).unwrap();
    assert_eq!(session.state(), RecordingState::Failed);
    assert!(session.artifact().is_none());
    assert_eq!(session.lost_sources(), [SourceKind::Screen]);
    assert_eq!(
        source_lost_events(&h),
        [(SourceKind::Screen, "display-1".to_owned())]
    );
    assert_eq!(
        &h.log.events()[2..],
        ["event:source_lost", "event:failed"],
        "SourceLost precedes the outcome"
    );
    let calls = h.log.calls();
    let tail: Vec<&str> = calls
        .iter()
        .skip_while(|c| *c != "microphone.start")
        .skip(1)
        .map(String::as_str)
        .collect();
    assert_eq!(
        tail,
        [
            "screen.stop",
            "microphone.stop",
            "media.drain",
            "media.close"
        ],
        "remaining sources stop and media finalizes for recovery; nothing is published or aborted"
    );
    assert_eq!(h.app.stop(id(25)).unwrap(), RecordingState::Failed);
}

#[test]
fn optional_microphone_loss_continues_and_is_recorded_on_the_session() {
    let mut h = harness();
    h.app
        .prepare(request(26, RecordingMode::Screen, true))
        .unwrap();
    h.app.start(id(26)).unwrap();
    h.microphone.lose_all();
    assert_eq!(
        h.app.process_source_losses(id(26)).unwrap(),
        RecordingState::Recording
    );
    assert_eq!(
        source_lost_events(&h),
        [(SourceKind::Microphone, "mic-1".to_owned())]
    );
    // The loss is processed once; polling again changes nothing.
    let before = h.log.entries();
    assert_eq!(
        h.app.process_source_losses(id(26)).unwrap(),
        RecordingState::Recording
    );
    assert_eq!(h.log.entries(), before);

    assert_eq!(h.app.stop(id(26)).unwrap(), RecordingState::Completed);
    let session = h.app.session(id(26)).unwrap();
    assert_eq!(session.lost_sources(), [SourceKind::Microphone]);
    assert!(session.artifact().is_some());
    // The lost microphone was stopped when the loss was processed, not again at stop.
    let stops = h
        .log
        .calls()
        .into_iter()
        .filter(|c| c == "microphone.stop")
        .count();
    assert_eq!(stops, 1);
}

#[test]
fn source_loss_seen_only_at_stop_still_prevents_publication() {
    let mut h = harness();
    h.app
        .prepare(request(27, RecordingMode::Webcam, false))
        .unwrap();
    h.app.start(id(27)).unwrap();
    h.camera.lose_all();
    let error = h.app.stop(id(27)).unwrap_err();
    assert!(
        matches!(
            error,
            ApplicationError::SourceLost {
                kind: SourceKind::Camera,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(!h.log.calls().contains(&"storage.publish".to_string()));
    assert_eq!(h.app.state(id(27)).unwrap(), RecordingState::Failed);
    assert_eq!(source_lost_events(&h).len(), 1);
}

#[test]
fn source_losses_are_ignored_outside_capture() {
    let mut h = harness();
    h.app
        .prepare(request(28, RecordingMode::Screen, false))
        .unwrap();
    let before = h.log.entries();
    assert_eq!(
        h.app.process_source_losses(id(28)).unwrap(),
        RecordingState::Ready
    );
    assert_eq!(h.log.entries(), before);
    assert_eq!(
        h.app.process_source_losses(id(98)),
        Err(ApplicationError::RecordingNotFound(id(98)))
    );
}

#[test]
fn lost_source_stop_failure_is_reported_when_capture_continues() {
    let mut h = harness();
    h.faults.add("microphone.stop");
    h.app
        .prepare(request(29, RecordingMode::Screen, true))
        .unwrap();
    h.app.start(id(29)).unwrap();
    h.microphone.lose_all();
    match h.app.process_source_losses(id(29)).unwrap_err() {
        ApplicationError::CleanupIncomplete { state, failures } => {
            assert_eq!(state, RecordingState::Recording);
            assert_eq!(
                failures[0].capability,
                RecordingCapability::MicrophoneCapture
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(h.app.state(id(29)).unwrap(), RecordingState::Recording);
}

#[test]
fn foreign_closed_output_is_never_published() {
    let mut h = harness();
    h.faults.add("media.close.foreign");
    h.app
        .prepare(request(30, RecordingMode::Screen, false))
        .unwrap();
    h.app.start(id(30)).unwrap();
    let error = h.app.stop(id(30)).unwrap_err();
    assert_eq!(
        failed_capability(&error),
        Some(RecordingCapability::MediaPipeline)
    );
    assert!(!h.log.calls().contains(&"storage.publish".to_string()));
    assert_eq!(h.app.state(id(30)).unwrap(), RecordingState::Failed);
}

#[test]
fn media_sinks_that_do_not_match_the_configuration_are_rejected_and_rolled_back() {
    let mut h = harness();
    h.faults.add("media.prepare.mismatch");
    let error = h
        .app
        .prepare(request(31, RecordingMode::Screen, false))
        .unwrap_err();
    assert_eq!(
        failed_capability(&error),
        Some(RecordingCapability::MediaPipeline)
    );
    assert_eq!(
        h.log.calls(),
        [
            "storage.begin",
            "media.prepare",
            "media.abort",
            "storage.abort"
        ]
    );
    assert_eq!(
        h.app.state(id(31)),
        Err(ApplicationError::RecordingNotFound(id(31)))
    );
}

#[test]
fn source_lost_event_names_the_selected_source() {
    let mut h = harness();
    h.app
        .prepare(request(32, RecordingMode::Microphone, false))
        .unwrap();
    h.app.start(id(32)).unwrap();
    h.microphone.lose_all();
    assert!(h.app.process_source_losses(id(32)).is_err());
    let lost: Vec<SourceId> = h
        .events
        .history()
        .into_iter()
        .filter_map(|event| match event {
            RecordingEvent::SourceLost {
                recording_id,
                source,
                ..
            } if recording_id == id(32) => Some(source),
            _ => None,
        })
        .collect();
    assert_eq!(lost, [SourceId::new("mic-1").unwrap()]);
}

#[test]
fn rejected_loss_does_not_hide_other_losses_and_stays_tracked() {
    let mut h = harness();
    h.app
        .prepare(request(33, RecordingMode::Screen, true))
        .unwrap();
    h.app.start(id(33)).unwrap();
    // A loss the aggregate must reject (camera is not part of this recording),
    // placed ahead of a genuine microphone loss.
    let (sink, _stream) = crate::data_plane::video_channel(1).unwrap();
    let monitor = sink.monitor();
    sink.report_source_lost();
    h.app.sessions[0]
        .resources
        .as_mut()
        .unwrap()
        .captures
        .insert(
            0,
            StartedCapture {
                kind: SourceKind::Camera,
                source: SourceId::new("camera-x").unwrap(),
                session: CaptureSessionId::new("camera-x").unwrap(),
                monitor,
            },
        );
    h.microphone.lose_all();

    let rejected = Err(ApplicationError::Domain(DomainError::UnexpectedSource(
        SourceKind::Camera,
    )));
    assert_eq!(h.app.process_source_losses(id(33)), rejected);
    let session = h.app.session(id(33)).unwrap();
    assert_eq!(session.state(), RecordingState::Recording);
    assert_eq!(session.lost_sources(), [SourceKind::Microphone]);
    assert_eq!(
        source_lost_events(&h),
        [(SourceKind::Microphone, "mic-1".to_owned())]
    );

    // The rejected loss is still tracked; the recorded one is not repeated.
    assert_eq!(h.app.process_source_losses(id(33)), rejected);
    assert_eq!(source_lost_events(&h).len(), 1);
    let microphone_stops = h
        .log
        .calls()
        .into_iter()
        .filter(|c| c == "microphone.stop")
        .count();
    assert_eq!(microphone_stops, 1);
}
