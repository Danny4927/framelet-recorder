#![forbid(unsafe_code)]

//! Outbound ports owned by the Recording application core.
//!
//! Implementations are adapters. All methods are invoked by application workers,
//! never by UI or real-time device callbacks. Implementations must document their
//! concrete thread-safety and cancellation guarantees.

use std::path::{Path, PathBuf};
use std::time::Duration;

use framelet_capture_api::{SourceDescriptor, SourceId, SourceKind};

use crate::data_plane::{AudioSink, DataPlaneError, MediaSinks, VideoSink};
use crate::{RecordingArtifact, RecordingConfiguration, RecordingId};

/// Opaque identity for one opened capture source.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CaptureSessionId(String);

impl CaptureSessionId {
    /// Creates a non-empty adapter-neutral capture session identity.
    pub fn new(value: impl Into<String>) -> Result<Self, PortError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(PortError::InvalidInput(
                "capture session id must not be empty".into(),
            ));
        }
        Ok(Self(value))
    }

    /// Returns the adapter-neutral identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque identity for a prepared media output.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MediaSessionId(String);

impl MediaSessionId {
    /// Creates a non-empty media session identity.
    pub fn new(value: impl Into<String>) -> Result<Self, PortError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(PortError::InvalidInput(
                "media session id must not be empty".into(),
            ));
        }
        Ok(Self(value))
    }

    /// Returns the adapter-neutral identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A selected input source, resolved by the corresponding capture adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedSource {
    /// Adapter-local source identity.
    pub id: SourceId,
    /// Display name captured at selection time.
    pub display_name: String,
}

/// Typed adapter failure. Native error types must be translated at the boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortError {
    /// The user denied a required permission.
    PermissionDenied,
    /// The user cancelled a source picker.
    SelectionCancelled,
    /// A requested device or source disappeared.
    SourceUnavailable,
    /// The requested format or operation is unsupported.
    Unsupported(String),
    /// Input supplied by the application is invalid.
    InvalidInput(String),
    /// Storage is full or cannot accept more data.
    StorageExhausted,
    /// Adapter-specific operation failed.
    Failed(String),
}

impl std::fmt::Display for PortError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionDenied => write!(formatter, "permission denied"),
            Self::SelectionCancelled => write!(formatter, "source selection cancelled"),
            Self::SourceUnavailable => write!(formatter, "source unavailable"),
            Self::Unsupported(message) => write!(formatter, "unsupported operation: {message}"),
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::StorageExhausted => write!(formatter, "storage exhausted"),
            Self::Failed(message) => write!(formatter, "adapter operation failed: {message}"),
        }
    }
}

impl std::error::Error for PortError {}

impl From<DataPlaneError> for PortError {
    fn from(error: DataPlaneError) -> Self {
        Self::InvalidInput(error.to_string())
    }
}

/// Screen/window capture port. Methods run on a worker and must be cancellable.
///
/// Capture contract shared by all three capture ports (ADR-0020):
/// - `start` takes ownership of the core-owned sink for this source and
///   delivers owned payloads to it from the adapter's worker. If `start`
///   fails, the adapter must have released the sink before returning.
/// - When the device disappears, the adapter calls `report_source_lost` on
///   the sink; it never substitutes another source.
/// - `stop` stops the producer, joins its worker, and releases the sink
///   before returning. `stop` is idempotent: stopping a session that is not
///   active (already stopped, lost, or never started) succeeds without effect.
pub trait ScreenCapturePort: Send {
    /// Enumerates sources without starting capture.
    fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError>;
    /// Opens the OS-mediated picker and starts the selected source.
    fn start(
        &mut self,
        source: &SelectedSource,
        sink: VideoSink,
    ) -> Result<CaptureSessionId, PortError>;
    /// Stops the source and releases OS resources and the sink. Idempotent.
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError>;
}

/// Camera capture port. Device selection and native format negotiation stay in the adapter.
/// Follows the capture contract documented on [`ScreenCapturePort`].
pub trait CameraCapturePort: Send {
    /// Enumerates available cameras.
    fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError>;
    /// Opens the selected camera and delivers its frames to `sink`.
    fn start(
        &mut self,
        source: &SelectedSource,
        sink: VideoSink,
    ) -> Result<CaptureSessionId, PortError>;
    /// Stops the camera and releases device resources and the sink. Idempotent.
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError>;
}

/// Microphone capture port. Device callbacks must remain bounded and non-blocking;
/// a consumer worker, not the callback, offers blocks to the sink.
/// Follows the capture contract documented on [`ScreenCapturePort`].
pub trait MicrophoneCapturePort: Send {
    /// Enumerates available microphones.
    fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError>;
    /// Opens the selected microphone with an explicitly negotiated format.
    fn start(
        &mut self,
        source: &SelectedSource,
        sink: AudioSink,
    ) -> Result<CaptureSessionId, PortError>;
    /// Stops the microphone, joins/drains its worker, and releases the sink. Idempotent.
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError>;
}

/// Opaque identity for a storage reservation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorageSessionId(String);

impl StorageSessionId {
    /// Creates a non-empty storage session identity.
    pub fn new(value: impl Into<String>) -> Result<Self, PortError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(PortError::InvalidInput(
                "storage session id must not be empty".into(),
            ));
        }
        Ok(Self(value))
    }

    /// Returns the adapter-neutral identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where the media pipeline writes the output of one recording (ADR-0021).
///
/// Issued by the storage adapter inside a [`StorageReservation`] and forwarded
/// unchanged by the application to [`MediaPipelinePort::prepare`]. Core never
/// opens, inspects, or derives paths from it; it only compares targets.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OutputTarget(PathBuf);

impl OutputTarget {
    /// Creates a target from a non-empty location chosen by the storage adapter.
    pub fn new(location: impl Into<PathBuf>) -> Result<Self, PortError> {
        let location = location.into();
        if location.as_os_str().is_empty() {
            return Err(PortError::InvalidInput(
                "output target must not be empty".into(),
            ));
        }
        Ok(Self(location))
    }

    /// Returns the location the media writer must write to.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// An exclusive temporary output reserved by storage for one recording.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageReservation {
    /// Storage-side identity used for `publish` and `abort`.
    pub session: StorageSessionId,
    /// Where the media pipeline writes; handed to [`MediaPipelinePort::prepare`].
    pub target: OutputTarget,
}

/// Proof that the media writer for an [`OutputTarget`] has drained and closed.
///
/// [`RecordingStoragePort::publish`] requires it by value, and in the
/// application flow only [`MediaPipelinePort::close`] produces it. Because
/// [`ClosedOutput::new`] is public (media adapters live in other crates),
/// publish-after-close is enforced *by contract plus target check*, not by the
/// type alone: storage must reject a `ClosedOutput` whose target is not the one
/// it reserved for the session, and the application checks the target before
/// publishing. It is not `Clone`: one close permits one publication attempt.
#[derive(Debug, Eq, PartialEq)]
pub struct ClosedOutput {
    target: OutputTarget,
}

impl ClosedOutput {
    /// Declares that the writer for `target` has flushed and closed it.
    ///
    /// Constructed by media pipeline adapters at the end of a successful
    /// `close`, and by contract-test harnesses that stand in for a writer.
    #[must_use]
    pub const fn new(target: OutputTarget) -> Self {
        Self { target }
    }

    /// Returns the target whose writer has closed.
    #[must_use]
    pub const fn target(&self) -> &OutputTarget {
        &self.target
    }
}

/// A prepared media pipeline and the sinks its capture sources feed.
#[derive(Debug)]
pub struct PreparedMedia {
    /// Media-side identity used for all further pipeline calls.
    pub session: MediaSessionId,
    /// One bounded sink per source the configuration requires (ADR-0020).
    pub sinks: MediaSinks,
}

/// Media pipeline port: clock mapping, encoding, muxing, and finalization.
///
/// Payloads do not pass through this port; they arrive on the streams the
/// pipeline created with [`crate::data_plane::media_inputs`]. Before `start`,
/// the pipeline does not consume from its streams, so sinks fill up to their
/// capacity and then apply the overload policy.
pub trait MediaPipelinePort: Send {
    /// Creates an output pipeline writing to `target` and returns the sinks
    /// for exactly the sources `configuration` requires.
    fn prepare(
        &mut self,
        recording_id: RecordingId,
        configuration: &RecordingConfiguration,
        target: &OutputTarget,
    ) -> Result<PreparedMedia, PortError>;
    /// Starts consuming the input streams.
    fn start(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
    /// Pauses media ingestion without discarding already accepted blocks.
    fn pause(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
    /// Resumes media ingestion.
    fn resume(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
    /// Consumes every stream until it ended and encodes all accepted media.
    ///
    /// After an audio stream ended, the pipeline must read its final
    /// `stats().pending_gap` and account for that trailing loss like a marked
    /// [`crate::data_plane::AudioGap`]; it is never silently ignored.
    fn drain(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
    /// Finalizes the container, closes the writer, and releases the session.
    ///
    /// The session is released whether or not finalization succeeds. Only a
    /// successful close returns the [`ClosedOutput`] that permits publication.
    fn close(&mut self, session: &MediaSessionId) -> Result<ClosedOutput, PortError>;
    /// Releases the pipeline without producing a closed output (cancel and
    /// rollback). Partial output is left to storage. Idempotent: aborting a
    /// session that is not active succeeds without effect.
    fn abort(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
}

/// Storage port. Implementations must use temporary output and publish atomically where supported.
pub trait RecordingStoragePort: Send {
    /// Reserves an exclusive temporary output for a recording.
    fn begin(
        &mut self,
        recording_id: RecordingId,
        configuration: &RecordingConfiguration,
    ) -> Result<StorageReservation, PortError>;
    /// Publishes the closed output of a reservation under its final name.
    ///
    /// Must fail with [`PortError::InvalidInput`], publishing nothing, when
    /// `output` names another target than the one reserved for `session` or
    /// when `session` is not an active reservation.
    fn publish(
        &mut self,
        session: &StorageSessionId,
        output: ClosedOutput,
    ) -> Result<RecordingArtifact, PortError>;
    /// Removes incomplete temporary output; must not delete an existing or
    /// published recording. Idempotent: aborting a session that is not active
    /// (already aborted, already published, or unknown) succeeds without effect.
    fn abort(&mut self, session: &StorageSessionId) -> Result<(), PortError>;
}

/// Monotonic time source used for ordering and elapsed-time measurement.
pub trait MonotonicClock: Send + Sync {
    /// Returns time relative to an arbitrary monotonic origin.
    fn now(&self) -> Duration;
}

/// Persisted, user-editable settings relevant to recording.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingSettings {
    /// Preferred output directory as an opaque user-facing path.
    pub output_directory: String,
    /// Whether microphone input is enabled by default where the mode permits it.
    pub microphone_enabled_by_default: bool,
}

/// Settings port. Implementations own serialization and platform-specific locations.
pub trait RecordingSettingsPort: Send + Sync {
    /// Loads settings, returning defaults when no settings have been stored.
    fn load(&self) -> Result<RecordingSettings, PortError>;
    /// Validates and stores settings.
    fn save(&self, settings: &RecordingSettings) -> Result<(), PortError>;
}

/// Repository port for durable session metadata, not media payloads.
/// Media files are owned by RecordingStoragePort.
pub trait RecordingSessionRepository: Send + Sync {
    /// Inserts a new session snapshot; duplicate identifiers must be rejected.
    fn insert(&self, session: &crate::RecordingSession) -> Result<(), PortError>;
    /// Updates an existing session snapshot.
    fn update(&self, session: &crate::RecordingSession) -> Result<(), PortError>;
    /// Loads a session snapshot by identity.
    fn get(&self, id: RecordingId) -> Result<Option<crate::RecordingSession>, PortError>;
}

/// Domain facts emitted by the application after a state transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordingEvent {
    /// Preparation completed and the session can start.
    Ready {
        /// Identity of the session that finished preparation.
        recording_id: RecordingId,
    },
    /// Capture started.
    Started {
        /// Identity of the session that started capture.
        recording_id: RecordingId,
    },
    /// Capture paused.
    Paused {
        /// Identity of the session that was paused.
        recording_id: RecordingId,
    },
    /// Capture resumed.
    Resumed {
        /// Identity of the session that resumed capture.
        recording_id: RecordingId,
    },
    /// Finalization completed and the artifact is available.
    Completed {
        /// Identity of the completed session.
        recording_id: RecordingId,
        /// Published output descriptor.
        artifact: RecordingArtifact,
    },
    /// Session failed.
    Failed {
        /// Identity of the failed session.
        recording_id: RecordingId,
        /// Human-readable, adapter-neutral failure summary.
        reason: String,
    },
    /// Preparation was cancelled.
    Cancelled {
        /// Identity of the session that was cancelled.
        recording_id: RecordingId,
    },
    /// A source was lost during capture. Published before the outcome
    /// (continued degraded, or `Failed`) takes effect.
    SourceLost {
        /// Identity of the session affected by source loss.
        recording_id: RecordingId,
        /// Kind of the lost source.
        kind: SourceKind,
        /// Source identity that became unavailable.
        source: SourceId,
    },
}

/// Outbound event publisher. Publishing must not block capture callbacks.
pub trait RecordingEventPublisher: Send + Sync {
    /// Publishes a domain fact to application observers.
    fn publish(&self, event: RecordingEvent);
}

/// Snapshot of application-level metrics, not a high-frequency media callback payload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecordingMetrics {
    /// Elapsed monotonic recording duration in milliseconds.
    pub elapsed_millis: u64,
    /// Number of audio blocks rejected due to backpressure.
    pub rejected_audio_blocks: u64,
    /// Number of video frames dropped by a bounded transport.
    pub dropped_video_frames: u64,
    /// Number of source disconnects observed.
    pub source_disconnects: u64,
}

/// Metrics query port. Implementations should coalesce high-rate updates.
pub trait RecordingMetricsPort: Send + Sync {
    /// Returns the latest metrics snapshot for a session.
    fn snapshot(&self, recording_id: RecordingId) -> Result<RecordingMetrics, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_session_ids_reject_empty_values() {
        assert!(CaptureSessionId::new(" ").is_err());
        assert!(MediaSessionId::new("").is_err());
        assert!(StorageSessionId::new("\t").is_err());
        assert!(OutputTarget::new("").is_err());
    }

    #[test]
    fn closed_output_names_its_target() {
        let target = OutputTarget::new("out/candidate.part").unwrap();
        let closed = ClosedOutput::new(target.clone());
        assert_eq!(closed.target(), &target);
        assert_eq!(target.as_path(), Path::new("out/candidate.part"));
    }

    #[test]
    fn opaque_session_ids_preserve_values() {
        assert_eq!(
            CaptureSessionId::new("screen-1").unwrap().as_str(),
            "screen-1"
        );
        assert_eq!(MediaSessionId::new("media-1").unwrap().as_str(), "media-1");
    }

    #[test]
    fn metrics_snapshot_defaults_to_zero() {
        assert_eq!(RecordingMetrics::default().rejected_audio_blocks, 0);
        assert_eq!(RecordingMetrics::default().dropped_video_frames, 0);
    }

    #[test]
    fn storage_session_id_preserves_value() {
        assert_eq!(
            StorageSessionId::new("storage-1").unwrap().as_str(),
            "storage-1"
        );
    }

    #[test]
    fn port_errors_have_descriptive_messages() {
        assert_eq!(PortError::StorageExhausted.to_string(), "storage exhausted");
        assert_eq!(
            PortError::Failed("disk".into()).to_string(),
            "adapter operation failed: disk"
        );
    }
}
