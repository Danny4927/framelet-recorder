//! Contract suite for [`MediaPipelinePort`] and the sinks it prepares
//! (ADR-0020, ADR-0021).
//!
//! The overload suites rely on the port contract that a pipeline does not
//! consume from its streams before `start`, so a sink fills deterministically.

use crate::contract::fakes::{synthetic_block, synthetic_frame};
use crate::data_plane::{AudioDelivery, AudioGap, MediaSinks, StreamEnd, VideoDelivery};
use crate::domain::{RecordingConfiguration, RecordingId, RecordingMode, SourceKind};
use crate::ports::{MediaPipelinePort, MediaSessionId, OutputTarget, PortError, PreparedMedia};

/// A media pipeline under test.
pub trait MediaSubject {
    /// Returns the adapter under test.
    fn media(&mut self) -> &mut dyn MediaPipelinePort;
    /// Returns a fresh output target the pipeline may write to.
    fn target(&mut self) -> OutputTarget;
}

const ALL_CONFIGURATIONS: [(RecordingMode, bool); 7] = [
    (RecordingMode::Screen, false),
    (RecordingMode::Screen, true),
    (RecordingMode::Microphone, false),
    (RecordingMode::Webcam, false),
    (RecordingMode::Webcam, true),
    (RecordingMode::ScreenAndWebcam, false),
    (RecordingMode::ScreenAndWebcam, true),
];

const KINDS: [SourceKind; 3] = [
    SourceKind::Screen,
    SourceKind::Camera,
    SourceKind::Microphone,
];

fn configuration(mode: RecordingMode, microphone: bool) -> RecordingConfiguration {
    match RecordingConfiguration::new(mode, microphone) {
        Ok(configuration) => configuration,
        Err(error) => panic!("valid configuration rejected: {error}"),
    }
}

fn prepare<S: MediaSubject>(
    subject: &mut S,
    id: u64,
    configuration: &RecordingConfiguration,
) -> (PreparedMedia, OutputTarget) {
    let target = subject.target();
    let prepared = subject
        .media()
        .prepare(RecordingId::new(id), configuration, &target)
        .unwrap_or_else(|error| panic!("prepare failed: {error}"));
    (prepared, target)
}

fn is_invalid_input<T: std::fmt::Debug>(result: &Result<T, PortError>) -> bool {
    matches!(result, Err(PortError::InvalidInput(_)))
}

fn assert_capacity(capacity: usize, maximum: usize) {
    assert!(
        (1..=maximum).contains(&capacity),
        "sink capacity {capacity} outside 1..={maximum}"
    );
}

/// For every mode and microphone choice, `prepare` returns sinks for exactly
/// the required sources, each with a valid bounded capacity.
pub fn prepares_sinks_for_exactly_the_required_sources<S: MediaSubject>(
    mut make: impl FnMut() -> S,
) {
    for (index, (mode, microphone)) in (1_u64..).zip(ALL_CONFIGURATIONS) {
        let mut subject = make();
        let configuration = configuration(mode, microphone);
        let (mut prepared, _) = prepare(&mut subject, index, &configuration);
        assert!(
            prepared.sinks.matches(&configuration),
            "{mode:?} microphone={microphone}: sinks do not match"
        );
        for kind in KINDS {
            assert_eq!(
                prepared.sinks.has(kind),
                configuration.requires_source(kind),
                "{mode:?} microphone={microphone}: {kind:?}"
            );
        }
        check_capacities(&mut prepared.sinks);
        assert_eq!(subject.media().abort(&prepared.session), Ok(()));
    }
}

fn check_capacities(sinks: &mut MediaSinks) {
    for kind in [SourceKind::Screen, SourceKind::Camera] {
        if let Some(sink) = sinks.take_video(kind) {
            assert_capacity(sink.capacity(), crate::data_plane::MAX_VIDEO_QUEUE_CAPACITY);
        }
    }
    if let Some(sink) = sinks.take_audio() {
        assert_capacity(sink.capacity(), crate::data_plane::MAX_AUDIO_QUEUE_CAPACITY);
    }
}

/// prepare → start → pause → resume → (producers gone) → drain → close
/// returns the closed output for exactly the prepared target and releases the
/// session: later calls fail with `InvalidInput`, and sinks report `Closed`.
pub fn lifecycle_closes_to_the_prepared_target<S: MediaSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let configuration = configuration(RecordingMode::Screen, true);
    let (mut prepared, target) = prepare(&mut subject, 10, &configuration);
    let session = prepared.session.clone();
    let mut video = prepared
        .sinks
        .take_video(SourceKind::Screen)
        .unwrap_or_else(|| panic!("screen sink missing"));
    let audio = prepared
        .sinks
        .take_audio()
        .unwrap_or_else(|| panic!("microphone sink missing"));
    let video_monitor = video.monitor();
    let media = subject.media();
    assert_eq!(media.start(&session), Ok(()));
    assert_eq!(video.try_send(synthetic_frame(0)), VideoDelivery::Accepted);
    assert_eq!(media.pause(&session), Ok(()));
    assert_eq!(media.resume(&session), Ok(()));
    // `drain` consumes until every stream has ended, so producers are
    // released first, as the application does by stopping every capture.
    drop(audio);
    drop(video);
    assert_eq!(video_monitor.stats().end, Some(StreamEnd::Stopped));
    assert_eq!(media.drain(&session), Ok(()));
    let closed = media
        .close(&session)
        .unwrap_or_else(|error| panic!("close failed: {error}"));
    assert_eq!(
        closed.target(),
        &target,
        "closed output must name the prepared target"
    );
    assert!(
        video_monitor.stats().consumer_closed,
        "close must release the input streams"
    );
    let again = media.close(&session);
    assert!(is_invalid_input(&again), "close after close: {again:?}");
    let start = media.start(&session);
    assert!(is_invalid_input(&start), "start after close: {start:?}");
    assert_eq!(
        media.abort(&session),
        Ok(()),
        "abort after close is a no-op"
    );
}

/// Abort releases the pipeline and its streams, can be repeated, and leaves
/// nothing that can still be closed.
pub fn abort_is_idempotent<S: MediaSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let configuration = configuration(RecordingMode::Microphone, false);
    let (mut prepared, _) = prepare(&mut subject, 11, &configuration);
    let mut audio = prepared
        .sinks
        .take_audio()
        .unwrap_or_else(|| panic!("microphone sink missing"));
    let media = subject.media();
    assert_eq!(media.abort(&prepared.session), Ok(()));
    assert_eq!(
        media.abort(&prepared.session),
        Ok(()),
        "abort must be idempotent"
    );
    assert_eq!(
        audio.try_send(synthetic_block(0)),
        AudioDelivery::Closed,
        "abort must release the input streams"
    );
    let closed = media.close(&prepared.session);
    assert!(is_invalid_input(&closed), "close after abort: {closed:?}");
}

/// Every session-taking method fails with `InvalidInput` for a session the
/// pipeline never prepared, except `abort`, which is a no-op.
pub fn rejects_unknown_session<S: MediaSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let unknown = match MediaSessionId::new("never-prepared") {
        Ok(session) => session,
        Err(error) => panic!("valid session id rejected: {error}"),
    };
    let media = subject.media();
    for (name, result) in [
        ("start", media.start(&unknown)),
        ("pause", media.pause(&unknown)),
        ("resume", media.resume(&unknown)),
        ("drain", media.drain(&unknown)),
    ] {
        assert!(is_invalid_input(&result), "{name}: {result:?}");
    }
    let closed = media.close(&unknown);
    assert!(is_invalid_input(&closed), "close: {closed:?}");
    assert_eq!(media.abort(&unknown), Ok(()));
}

/// Offering more frames than the video capacity drops the oldest frames,
/// counts every drop, and keeps the queue at capacity (REQ-FLOW-01/02).
pub fn video_overload_counts_drops<S: MediaSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let configuration = configuration(RecordingMode::Screen, false);
    let (mut prepared, _) = prepare(&mut subject, 12, &configuration);
    let mut sink = prepared
        .sinks
        .take_video(SourceKind::Screen)
        .unwrap_or_else(|| panic!("screen sink missing"));
    let capacity = sink.capacity();
    let monitor = sink.monitor();
    let overload: u64 = 3;
    let total = u64::try_from(capacity)
        .unwrap_or(u64::MAX)
        .saturating_add(overload);
    let mut dropped = 0;
    for sequence in 0..total {
        match sink.try_send(synthetic_frame(sequence)) {
            VideoDelivery::Accepted => {}
            VideoDelivery::DroppedStale { dropped_total } => {
                dropped += 1;
                assert_eq!(dropped_total, dropped, "drop counter must count every drop");
            }
            VideoDelivery::Closed => panic!("pipeline closed its stream before start"),
        }
        assert!(
            monitor.stats().queued <= capacity,
            "queue exceeded capacity"
        );
    }
    let stats = monitor.stats();
    assert_eq!(dropped, overload, "exactly the overflow must be dropped");
    assert_eq!(stats.dropped, overload);
    assert_eq!(stats.queued, capacity);
    assert_eq!(stats.high_water, capacity);
    drop(sink);
    assert_eq!(subject.media().abort(&prepared.session), Ok(()));
}

/// Offering more blocks than the audio capacity rejects the newest blocks,
/// keeps every accepted block, and reports one explicit discontinuity
/// (REQ-FLOW-01/03).
pub fn audio_overload_yields_discontinuity<S: MediaSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let configuration = configuration(RecordingMode::Microphone, false);
    let (mut prepared, _) = prepare(&mut subject, 13, &configuration);
    let mut sink = prepared
        .sinks
        .take_audio()
        .unwrap_or_else(|| panic!("microphone sink missing"));
    let capacity = sink.capacity();
    let monitor = sink.monitor();
    let overload: u64 = 2;
    let total = u64::try_from(capacity)
        .unwrap_or(u64::MAX)
        .saturating_add(overload);
    let mut rejected = 0;
    for sequence in 0..total {
        match sink.try_send(synthetic_block(sequence)) {
            AudioDelivery::Accepted => {}
            AudioDelivery::Discontinuity { lost_blocks_total } => {
                rejected += 1;
                assert_eq!(
                    lost_blocks_total, rejected,
                    "every rejection must be counted"
                );
            }
            AudioDelivery::Closed => panic!("pipeline closed its stream before start"),
        }
        assert!(
            monitor.stats().queued <= capacity,
            "queue exceeded capacity"
        );
    }
    let stats = monitor.stats();
    assert_eq!(rejected, overload, "exactly the overflow must be rejected");
    assert_eq!(stats.dropped, overload);
    assert_eq!(stats.discontinuities, 1, "one overload episode, one gap");
    assert_eq!(stats.lost_audio_frames, overload.saturating_mul(480));
    assert_eq!(
        stats.accepted,
        u64::try_from(capacity).unwrap_or(u64::MAX),
        "accepted audio must be retained"
    );
    drop(sink);
    assert_eq!(subject.media().abort(&prepared.session), Ok(()));
}

/// Audio rejected after the last accepted block is still reported once the
/// stream has ended: no packet carries it, so the final stream stats expose it
/// as `pending_gap`, which the pipeline must account for (REQ-FLOW-03).
pub fn audio_trailing_loss_is_reported_after_stream_end<S: MediaSubject>(
    mut make: impl FnMut() -> S,
) {
    let mut subject = make();
    let configuration = configuration(RecordingMode::Microphone, false);
    let (mut prepared, _) = prepare(&mut subject, 15, &configuration);
    let mut sink = prepared
        .sinks
        .take_audio()
        .unwrap_or_else(|| panic!("microphone sink missing"));
    let capacity = u64::try_from(sink.capacity()).unwrap_or(u64::MAX);
    let monitor = sink.monitor();
    let overload: u64 = 3;
    for sequence in 0..capacity.saturating_add(overload) {
        let _delivery = sink.try_send(synthetic_block(sequence));
    }
    // The producer stops right after the overload: the loss is trailing.
    drop(sink);
    let stats = monitor.stats();
    assert_eq!(stats.end, Some(StreamEnd::Stopped));
    assert_eq!(
        stats.pending_gap,
        Some(AudioGap {
            lost_blocks: overload,
            lost_frames: overload.saturating_mul(480)
        }),
        "trailing audio loss must remain visible after the stream ended"
    );
    assert_eq!(stats.dropped, overload);
    assert_eq!(stats.discontinuities, 1);
    assert_eq!(subject.media().abort(&prepared.session), Ok(()));
}

/// For the configuration with every source, flooding each sink with three
/// times its capacity never lets a queue exceed its capacity (REQ-FLOW-01).
pub fn queues_never_exceed_capacity<S: MediaSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let configuration = configuration(RecordingMode::ScreenAndWebcam, true);
    let (mut prepared, _) = prepare(&mut subject, 14, &configuration);
    for kind in [SourceKind::Screen, SourceKind::Camera] {
        let mut sink = prepared
            .sinks
            .take_video(kind)
            .unwrap_or_else(|| panic!("{kind:?} sink missing"));
        let monitor = sink.monitor();
        for sequence in 0..u64::try_from(sink.capacity().saturating_mul(3)).unwrap_or(0) {
            let _delivery = sink.try_send(synthetic_frame(sequence));
            let stats = monitor.stats();
            assert!(stats.queued <= stats.capacity, "{kind:?} exceeded capacity");
        }
        assert!(monitor.stats().high_water <= sink.capacity());
    }
    let mut sink = prepared
        .sinks
        .take_audio()
        .unwrap_or_else(|| panic!("microphone sink missing"));
    let monitor = sink.monitor();
    for sequence in 0..u64::try_from(sink.capacity().saturating_mul(3)).unwrap_or(0) {
        let _delivery = sink.try_send(synthetic_block(sequence));
        let stats = monitor.stats();
        assert!(
            stats.queued <= stats.capacity,
            "microphone exceeded capacity"
        );
    }
    assert!(monitor.stats().high_water <= sink.capacity());
    drop(sink);
    assert_eq!(subject.media().abort(&prepared.session), Ok(()));
}

/// Runs every media suite.
pub fn run_all<S: MediaSubject>(mut make: impl FnMut() -> S) {
    prepares_sinks_for_exactly_the_required_sources(&mut make);
    lifecycle_closes_to_the_prepared_target(&mut make);
    abort_is_idempotent(&mut make);
    rejects_unknown_session(&mut make);
    video_overload_counts_drops(&mut make);
    audio_overload_yields_discontinuity(&mut make);
    audio_trailing_loss_is_reported_after_stream_end(&mut make);
    queues_never_exceed_capacity(&mut make);
}
