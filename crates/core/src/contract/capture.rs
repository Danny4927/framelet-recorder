//! Contract suite for the capture ports (ADR-0020).
//!
//! Screen, camera, and microphone ports share one contract; a subject adapts
//! a concrete port and supplies a scripted way to produce media and to lose
//! the source (a synthetic source or a fault seam in a real adapter).

use crate::data_plane::{
    AudioSink, AudioStream, StreamEnd, StreamMonitor, VideoSink, VideoStream, audio_channel,
    video_channel,
};
use crate::ports::{CaptureSessionId, PortError, SelectedSource};

/// Capacity of the sinks the capture suites hand to an adapter; large enough
/// that the suites never trigger the overload policy.
const SUITE_CAPACITY: usize = 16;

/// A sink type a capture port accepts. Implemented for [`VideoSink`] and
/// [`AudioSink`].
pub trait ContractSink: Send + Sized + 'static {
    /// Consuming end kept alive by the suite.
    type Stream: Send;
    /// Creates a bounded channel and a monitor for it.
    fn bounded(capacity: usize) -> (Self, Self::Stream, StreamMonitor);
}

impl ContractSink for VideoSink {
    type Stream = VideoStream;
    fn bounded(capacity: usize) -> (Self, Self::Stream, StreamMonitor) {
        match video_channel(capacity) {
            Ok((sink, stream)) => {
                let monitor = sink.monitor();
                (sink, stream, monitor)
            }
            Err(error) => panic!("suite capacity rejected: {error}"),
        }
    }
}

impl ContractSink for AudioSink {
    type Stream = AudioStream;
    fn bounded(capacity: usize) -> (Self, Self::Stream, StreamMonitor) {
        match audio_channel(capacity) {
            Ok((sink, stream)) => {
                let monitor = sink.monitor();
                (sink, stream, monitor)
            }
            Err(error) => panic!("suite capacity rejected: {error}"),
        }
    }
}

/// A capture adapter under test.
pub trait CaptureSubject {
    /// Sink type of the adapter's port.
    type Sink: ContractSink;
    /// Returns a source the adapter can start.
    fn source(&mut self) -> SelectedSource;
    /// Returns a source that does not exist.
    fn missing_source(&mut self) -> SelectedSource;
    /// Calls the port's `start`.
    fn start(
        &mut self,
        source: &SelectedSource,
        sink: Self::Sink,
    ) -> Result<CaptureSessionId, PortError>;
    /// Calls the port's `stop`.
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError>;
    /// Makes the source of `session` produce `count` media items.
    fn produce(&mut self, session: &CaptureSessionId, count: usize);
    /// Makes the source of `session` disappear.
    fn lose_source(&mut self, session: &CaptureSessionId);
}

fn started<S: CaptureSubject>(
    subject: &mut S,
) -> (
    CaptureSessionId,
    StreamMonitor,
    <S::Sink as ContractSink>::Stream,
) {
    let (sink, stream, monitor) = S::Sink::bounded(SUITE_CAPACITY);
    let source = subject.source();
    let session = subject
        .start(&source, sink)
        .unwrap_or_else(|error| panic!("start failed: {error}"));
    (session, monitor, stream)
}

/// Produced media reaches the sink in order and stop ends the stream.
pub fn delivers_produced_media_to_its_sink<S: CaptureSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let (session, monitor, _stream) = started(&mut subject);
    subject.produce(&session, 3);
    let stats = monitor.stats();
    assert_eq!(stats.accepted, 3, "every produced item must reach the sink");
    assert_eq!(stats.end, None, "stream must stay open while capturing");
    assert_eq!(subject.stop(&session), Ok(()));
    assert_eq!(monitor.stats().end, Some(StreamEnd::Stopped));
}

/// Stop releases the sink before returning and can be repeated (REQ-LIFE-11).
pub fn stop_is_idempotent_and_releases_sink<S: CaptureSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let (session, monitor, _stream) = started(&mut subject);
    assert_eq!(subject.stop(&session), Ok(()));
    assert_eq!(
        monitor.stats().end,
        Some(StreamEnd::Stopped),
        "stop must release the sink before returning"
    );
    assert_eq!(subject.stop(&session), Ok(()), "second stop must succeed");
    subject.produce(&session, 1);
    assert_eq!(
        monitor.stats().accepted,
        0,
        "nothing is delivered after stop"
    );
}

/// A start that fails reports a typed error and releases the sink.
pub fn failed_start_is_typed_and_releases_sink<S: CaptureSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let (sink, _stream, monitor) = S::Sink::bounded(SUITE_CAPACITY);
    let missing = subject.missing_source();
    assert_eq!(
        subject.start(&missing, sink),
        Err(PortError::SourceUnavailable),
        "a missing source must be reported as SourceUnavailable"
    );
    assert_eq!(
        monitor.stats().end,
        Some(StreamEnd::Stopped),
        "a failed start must not keep the sink"
    );
}

/// Source loss is reported through the sink, never silently, and a later
/// stop still succeeds.
pub fn source_loss_is_reported_through_the_sink<S: CaptureSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let (session, monitor, _stream) = started(&mut subject);
    subject.produce(&session, 1);
    subject.lose_source(&session);
    assert!(
        monitor.source_lost(),
        "loss must end the stream as SourceLost"
    );
    assert_eq!(monitor.stats().accepted, 1, "media before the loss is kept");
    assert_eq!(
        subject.stop(&session),
        Ok(()),
        "stop after loss must succeed"
    );
    assert_eq!(monitor.stats().end, Some(StreamEnd::SourceLost));
}

/// Two sessions of the same adapter get distinct identities.
pub fn sessions_are_distinct<S: CaptureSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let (first, _, _first_stream) = started(&mut subject);
    assert_eq!(subject.stop(&first), Ok(()));
    let (second, _, _second_stream) = started(&mut subject);
    assert_ne!(first, second, "capture session ids must not be reused");
    assert_eq!(subject.stop(&second), Ok(()));
}

/// Runs every capture suite.
pub fn run_all<S: CaptureSubject>(mut make: impl FnMut() -> S) {
    delivers_produced_media_to_its_sink(&mut make);
    stop_is_idempotent_and_releases_sink(&mut make);
    failed_start_is_typed_and_releases_sink(&mut make);
    source_loss_is_reported_through_the_sink(&mut make);
    sessions_are_distinct(&mut make);
}
