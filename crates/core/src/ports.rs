//! Outbound ports owned by the Recording application core.
//!
//! Implementations are adapters. All methods are invoked by application workers,
//! never by UI or real-time device callbacks. Implementations must document their
//! concrete thread-safety and cancellation guarantees.

use std::time::Duration;

use framelet_capture_api::{AudioBlock, SourceDescriptor, SourceId, VideoFrame};

use crate::{RecordingArtifact, RecordingConfiguration, RecordingId, RecordingMode};

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

/// Screen/window capture port. Methods run on a worker and must be cancellable.
pub trait ScreenCapturePort: Send {
    /// Enumerates sources without starting capture.
    fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError>;
    /// Opens the OS-mediated picker and starts the selected source.
    fn start(&mut self, source: &SelectedSource) -> Result<CaptureSessionId, PortError>;
    /// Stops the source and releases OS resources. Must be safe to call once.
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError>;
}

/// Camera capture port. Device selection and native format negotiation stay in the adapter.
pub trait CameraCapturePort: Send {
    /// Enumerates available cameras.
    fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError>;
    /// Opens the selected camera.
    fn start(&mut self, source: &SelectedSource) -> Result<CaptureSessionId, PortError>;
    /// Stops the camera and releases device resources.
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError>;
}

/// Microphone capture port. Device callbacks must remain bounded and non-blocking.
pub trait MicrophoneCapturePort: Send {
    /// Enumerates available microphones.
    fn list_sources(&mut self) -> Result<Vec<SourceDescriptor>, PortError>;
    /// Opens the selected microphone with an explicitly negotiated format.
    fn start(&mut self, source: &SelectedSource) -> Result<CaptureSessionId, PortError>;
    /// Stops the microphone and joins/drains its worker before returning.
    fn stop(&mut self, session: &CaptureSessionId) -> Result<(), PortError>;
}

/// Media pipeline port. It accepts owned blocks and must preserve timestamps/order.
pub trait MediaPipelinePort: Send {
    /// Creates an output pipeline for the requested recording mode.
    fn prepare(
        &mut self,
        recording_id: RecordingId,
        mode: RecordingMode,
    ) -> Result<MediaSessionId, PortError>;
    /// Starts accepting media blocks.
    fn start(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
    /// Pauses media ingestion without discarding already accepted blocks.
    fn pause(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
    /// Resumes media ingestion.
    fn resume(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
    /// Transfers ownership of a video frame to the pipeline.
    fn push_video(&mut self, session: &MediaSessionId, frame: VideoFrame) -> Result<(), PortError>;
    /// Transfers ownership of an audio block to the pipeline.
    fn push_audio(&mut self, session: &MediaSessionId, block: AudioBlock) -> Result<(), PortError>;
    /// Stops accepting input and drains all previously accepted media.
    fn drain(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
    /// Releases pipeline resources after drain or abort.
    fn close(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
}

/// Storage port. Implementations must use temporary output and publish atomically where supported.
pub trait RecordingStoragePort: Send {
    /// Creates an exclusive temporary destination for a recording.
    fn begin(
        &mut self,
        recording_id: RecordingId,
        configuration: &RecordingConfiguration,
    ) -> Result<MediaSessionId, PortError>;
    /// Flushes and closes a finalized media output, then publishes it.
    fn publish(&mut self, session: &MediaSessionId) -> Result<RecordingArtifact, PortError>;
    /// Removes incomplete temporary output; must not delete an existing recording.
    fn abort(&mut self, session: &MediaSessionId) -> Result<(), PortError>;
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
    Ready { recording_id: RecordingId },
    /// Capture started.
    Started { recording_id: RecordingId },
    /// Capture paused.
    Paused { recording_id: RecordingId },
    /// Capture resumed.
    Resumed { recording_id: RecordingId },
    /// Finalization completed and the artifact is available.
    Completed {
        recording_id: RecordingId,
        artifact: RecordingArtifact,
    },
    /// Session failed.
    Failed {
        recording_id: RecordingId,
        reason: String,
    },
    /// Preparation was cancelled.
    Cancelled { recording_id: RecordingId },
    /// A source was lost during capture.
    SourceLost {
        recording_id: RecordingId,
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
}
