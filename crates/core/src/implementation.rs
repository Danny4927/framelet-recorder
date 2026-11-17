#![forbid(unsafe_code)]
//! Recording bounded-context domain model and application ports.
//!
//! The domain owns lifecycle invariants; adapters provide external resources.

use std::fmt;

/// Stable identity of a recording session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecordingId(u64);

impl RecordingId {
    /// Creates an identifier from a caller-provided unique value.
    #[must_use]
    pub const fn new(value: u64) -> Self { Self(value) }
    /// Returns the underlying identifier.
    #[must_use]
    pub const fn value(self) -> u64 { self.0 }
}

/// One of the four MVP recording modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingMode {
    /// Capture a screen or window.
    Screen,
    /// Capture microphone audio only.
    Microphone,
    /// Capture a webcam.
    Webcam,
    /// Capture screen with webcam picture-in-picture.
    ScreenAndWebcam,
}

/// Lifecycle of a recording session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingState {
    /// No active work has started.
    Idle,
    /// Sources and output are being prepared.
    Preparing,
    /// Sources and output are ready to start.
    Ready,
    /// Media is being captured.
    Recording,
    /// Capture is paused.
    Paused,
    /// Producers are stopped and output is being finalized.
    Finalizing,
    /// Output was finalized successfully.
    Completed,
    /// The session ended without a successful finalized artifact.
    Failed,
    /// The user cancelled before a recording artifact was completed.
    Cancelled,
}

/// Validated immutable configuration for a recording session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingConfiguration {
    mode: RecordingMode,
    include_microphone: bool,
}

impl RecordingConfiguration {
    /// Creates a configuration, rejecting microphone-only duplication.
    pub fn new(mode: RecordingMode, include_microphone: bool) -> Result<Self, DomainError> {
        if include_microphone && mode == RecordingMode::Microphone {
            return Err(DomainError::MicrophoneAlreadyIsRecordingSource);
        }
        Ok(Self { mode, include_microphone })
    }
    /// Returns the selected recording mode.
    #[must_use]
    pub const fn mode(&self) -> RecordingMode { self.mode }
    /// Returns whether a microphone is an optional additional source.
    #[must_use]
    pub const fn includes_microphone(&self) -> bool { self.include_microphone }
}

/// A finalized recording output descriptor owned by the application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingArtifact {
    location: String,
}

impl RecordingArtifact {
    /// Creates an artifact descriptor from a non-empty adapter-neutral location.
    pub fn new(location: String) -> Result<Self, DomainError> {
        if location.trim().is_empty() { return Err(DomainError::EmptyArtifactLocation); }
        Ok(Self { location })
    }
    /// Returns the adapter-neutral output location.
    #[must_use]
    pub fn location(&self) -> &str { &self.location }
}

/// Recording session aggregate; owns lifecycle invariants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingSession {
    id: RecordingId,
    configuration: RecordingConfiguration,
    state: RecordingState,
    artifact: Option<RecordingArtifact>,
}

impl RecordingSession {
    /// Starts a new session in the idle state.
    #[must_use]
    pub const fn new(id: RecordingId, configuration: RecordingConfiguration) -> Self {
        Self { id, configuration, state: RecordingState::Idle, artifact: None }
    }
    /// Returns the session identity.
    #[must_use]
    pub const fn id(&self) -> RecordingId { self.id }
    /// Returns the immutable session configuration.
    #[must_use]
    pub const fn configuration(&self) -> &RecordingConfiguration { &self.configuration }
    /// Returns the current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> RecordingState { self.state }
    /// Returns the finalized artifact, if finalization succeeded.
    #[must_use]
    pub const fn artifact(&self) -> Option<&RecordingArtifact> { self.artifact.as_ref() }
    /// Moves the session to preparing.
    pub fn begin_preparation(&mut self) -> Result<(), DomainError> { self.transition(RecordingState::Preparing) }
    /// Marks all required sources and output as ready.
    pub fn mark_ready(&mut self) -> Result<(), DomainError> { self.transition(RecordingState::Ready) }
    /// Starts media capture.
    pub fn start(&mut self) -> Result<(), DomainError> { self.transition(RecordingState::Recording) }
    /// Pauses media capture.
    pub fn pause(&mut self) -> Result<(), DomainError> { self.transition(RecordingState::Paused) }
    /// Resumes media capture.
    pub fn resume(&mut self) -> Result<(), DomainError> { self.transition(RecordingState::Recording) }
    /// Begins stopping producers and finalizing accepted media.
    pub fn begin_finalization(&mut self) -> Result<(), DomainError> { self.transition(RecordingState::Finalizing) }
    /// Marks finalization successful and publishes the artifact.
    pub fn complete(&mut self, artifact: RecordingArtifact) -> Result<(), DomainError> {
        self.require_state(RecordingState::Finalizing)?;
        self.artifact = Some(artifact);
        self.state = RecordingState::Completed;
        Ok(())
    }
    /// Marks the session failed. A failed session never exposes an artifact.
    pub fn fail(&mut self) -> Result<(), DomainError> {
        if matches!(self.state, RecordingState::Completed | RecordingState::Cancelled) {
            return Err(DomainError::InvalidTransition { from: self.state, to: RecordingState::Failed });
        }
        self.artifact = None;
        self.state = RecordingState::Failed;
        Ok(())
    }
    /// Cancels preparation or a ready session; active recordings must be stopped.
    pub fn cancel(&mut self) -> Result<(), DomainError> {
        if !matches!(self.state, RecordingState::Idle | RecordingState::Preparing | RecordingState::Ready) {
            return Err(DomainError::InvalidTransition { from: self.state, to: RecordingState::Cancelled });
        }
        self.state = RecordingState::Cancelled;
        Ok(())
    }
    /// Resets a terminal session for a new lifecycle attempt.
    pub fn reset(&mut self) -> Result<(), DomainError> {
        if !matches!(self.state, RecordingState::Completed | RecordingState::Failed | RecordingState::Cancelled) {
            return Err(DomainError::InvalidTransition { from: self.state, to: RecordingState::Idle });
        }
        self.artifact = None;
        self.state = RecordingState::Idle;
        Ok(())
    }
    fn transition(&mut self, to: RecordingState) -> Result<(), DomainError> {
        if !is_allowed_transition(self.state, to) {
            return Err(DomainError::InvalidTransition { from: self.state, to });
        }
        self.state = to;
        Ok(())
    }
    fn require_state(&self, expected: RecordingState) -> Result<(), DomainError> {
        if self.state == expected { Ok(()) }
        else { Err(DomainError::UnexpectedState { expected, actual: self.state }) }
    }
}

/// Determines legal lifecycle transitions for a recording session.
#[must_use]
pub const fn is_allowed_transition(from: RecordingState, to: RecordingState) -> bool {
    use RecordingState::{Cancelled, Completed, Failed, Finalizing, Idle, Paused, Preparing, Ready, Recording};
    matches!((from, to),
        (Idle, Preparing) | (Preparing, Ready) | (Preparing, Failed) | (Preparing, Cancelled)
        | (Ready, Recording) | (Ready, Failed) | (Ready, Cancelled)
        | (Recording, Paused) | (Recording, Finalizing) | (Recording, Failed)
        | (Paused, Recording) | (Paused, Finalizing) | (Paused, Failed)
        | (Finalizing, Completed) | (Finalizing, Failed)
        | (Completed, Idle) | (Failed, Idle) | (Cancelled, Idle))
}

/// Domain invariant or lifecycle violation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainError {
    /// Optional microphone selection duplicates the microphone-only mode.
    MicrophoneAlreadyIsRecordingSource,
    /// An output location must not be empty.
    EmptyArtifactLocation,
    /// A lifecycle transition is not permitted.
    InvalidTransition { from: RecordingState, to: RecordingState },
    /// An operation requires a specific lifecycle state.
    UnexpectedState { expected: RecordingState, actual: RecordingState },
}
impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MicrophoneAlreadyIsRecordingSource => write!(f, "microphone-only mode cannot add a second microphone source"),
            Self::EmptyArtifactLocation => write!(f, "recording artifact location must not be empty"),
            Self::InvalidTransition { from, to } => write!(f, "invalid recording transition from {from:?} to {to:?}"),
            Self::UnexpectedState { expected, actual } => write!(f, "recording operation requires {expected:?}, current state is {actual:?}"),
        }
    }
}
impl std::error::Error for DomainError {}

/// Request accepted by the prepare-recording inbound port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrepareRecording {
    /// Stable identity for the new session.
    pub recording_id: RecordingId,
    /// Validated recording configuration.
    pub configuration: RecordingConfiguration,
}

/// Typed application-level failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationError {
    /// Domain rejected the requested operation.
    Domain(DomainError),
    /// A required outbound capability failed.
    Port(String),
    /// A session identity is unknown to the application service.
    RecordingNotFound(RecordingId),
    /// A session already exists for the requested identity.
    RecordingAlreadyExists(RecordingId),
}
impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(f, "{error}"),
            Self::Port(message) => write!(f, "recording capability failed: {message}"),
            Self::RecordingNotFound(id) => write!(f, "recording {} was not found", id.value()),
            Self::RecordingAlreadyExists(id) => write!(f, "recording {} already exists", id.value()),
        }
    }
}
impl std::error::Error for ApplicationError {}
impl From<DomainError> for ApplicationError {
    fn from(error: DomainError) -> Self { Self::Domain(error) }
}

/// Outbound port for recording resources (capture and media/storage adapters).
pub trait RecordingResources: Send + Sync {
    /// Prepares all required sources and the output pipeline.
    fn prepare(&self, configuration: &RecordingConfiguration) -> Result<(), String>;
    /// Starts all prepared producers and the media pipeline.
    fn start(&self) -> Result<(), String>;
    /// Pauses capture and media processing.
    fn pause(&self) -> Result<(), String>;
    /// Resumes capture and media processing.
    fn resume(&self) -> Result<(), String>;
    /// Stops producers and drains all accepted media.
    fn stop_and_drain(&self) -> Result<(), String>;
    /// Finalizes output and returns its application-neutral location.
    fn finalize(&self) -> Result<RecordingArtifact, String>;
    /// Releases prepared resources without publishing an artifact.
    fn cancel(&self);
}

/// Inbound port implemented by the recording application service.
pub trait RecordingUseCases: Send + Sync {
    /// Prepares a recording session.
    fn prepare(&mut self, request: PrepareRecording) -> Result<RecordingState, ApplicationError>;
    /// Starts a prepared recording.
    fn start(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Pauses an active recording.
    fn pause(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Resumes a paused recording.
    fn resume(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Stops and finalizes an active or paused recording.
    fn stop(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Cancels preparation or a ready session.
    fn cancel(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Queries the current lifecycle state.
    fn state(&self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
}

/// Application service coordinating the Recording aggregate through an outbound port.
pub struct RecordingApplication<R> {
    resources: R,
    sessions: Vec<RecordingSession>,
}
impl<R: RecordingResources> RecordingApplication<R> {
    /// Creates an application service with an injected resource adapter.
    #[must_use]
    pub const fn new(resources: R) -> Self { Self { resources, sessions: Vec::new() } }
    fn session(&self, id: RecordingId) -> Result<&RecordingSession, ApplicationError> {
        self.sessions.iter().find(|s| s.id() == id).ok_or(ApplicationError::RecordingNotFound(id))
    }
    fn session_mut(&mut self, id: RecordingId) -> Result<&mut RecordingSession, ApplicationError> {
        self.sessions.iter_mut().find(|s| s.id() == id).ok_or(ApplicationError::RecordingNotFound(id))
    }
}
impl<R: RecordingResources> RecordingUseCases for RecordingApplication<R> {
    fn prepare(&mut self, request: PrepareRecording) -> Result<RecordingState, ApplicationError> {
        if self.sessions.iter().any(|s| s.id() == request.recording_id) {
            return Err(ApplicationError::RecordingAlreadyExists(request.recording_id));
        }
        let mut session = RecordingSession::new(request.recording_id, request.configuration);
        session.begin_preparation()?;
        if let Err(error) = self.resources.prepare(session.configuration()) {
            let _ = session.fail();
            return Err(ApplicationError::Port(error));
        }
        session.mark_ready()?;
        let state = session.state();
        self.sessions.push(session);
        Ok(state)
    }
    fn start(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        self.session(id)?.require_state(RecordingState::Ready)?;
        self.resources.start().map_err(ApplicationError::Port)?;
        let session = self.session_mut(id)?;
        session.start()?;
        Ok(session.state())
    }
    fn pause(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        self.session(id)?.require_state(RecordingState::Recording)?;
        self.resources.pause().map_err(ApplicationError::Port)?;
        let session = self.session_mut(id)?;
        session.pause()?;
        Ok(session.state())
    }
    fn resume(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        self.session(id)?.require_state(RecordingState::Paused)?;
        self.resources.resume().map_err(ApplicationError::Port)?;
        let session = self.session_mut(id)?;
        session.resume()?;
        Ok(session.state())
    }
    fn stop(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        let current = self.session(id)?.state();
        if !matches!(current, RecordingState::Recording | RecordingState::Paused) {
            return Err(ApplicationError::Domain(DomainError::UnexpectedState {
                expected: RecordingState::Recording, actual: current,
            }));
        }
        self.resources.stop_and_drain().map_err(ApplicationError::Port)?;
        self.session_mut(id)?.begin_finalization()?;
        match self.resources.finalize() {
            Ok(artifact) => self.session_mut(id)?.complete(artifact)?,
            Err(error) => {
                self.session_mut(id)?.fail()?;
                return Err(ApplicationError::Port(error));
            }
        }
        Ok(self.session(id)?.state())
    }
    fn cancel(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        let current = self.session(id)?.state();
        if !matches!(current, RecordingState::Preparing | RecordingState::Ready | RecordingState::Idle) {
            return Err(ApplicationError::Domain(DomainError::UnexpectedState {
                expected: RecordingState::Ready, actual: current,
            }));
        }
        self.resources.cancel();
        let session = self.session_mut(id)?;
        session.cancel()?;
        Ok(session.state())
    }
    fn state(&self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        Ok(self.session(id)?.state())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeResources {
        calls: Arc<Mutex<Vec<&'static str>>>,
        fail_finalize: bool,
    }
    impl RecordingResources for FakeResources {
        fn prepare(&self, _: &RecordingConfiguration) -> Result<(), String> { self.calls.lock().unwrap().push("prepare"); Ok(()) }
        fn start(&self) -> Result<(), String> { self.calls.lock().unwrap().push("start"); Ok(()) }
        fn pause(&self) -> Result<(), String> { self.calls.lock().unwrap().push("pause"); Ok(()) }
        fn resume(&self) -> Result<(), String> { self.calls.lock().unwrap().push("resume"); Ok(()) }
        fn stop_and_drain(&self) -> Result<(), String> { self.calls.lock().unwrap().push("stop_and_drain"); Ok(()) }
        fn finalize(&self) -> Result<RecordingArtifact, String> {
            self.calls.lock().unwrap().push("finalize");
            if self.fail_finalize { Err("injected finalization error".into()) }
            else { RecordingArtifact::new("recordings/session.framelet".into()).map_err(|e| e.to_string()) }
        }
        fn cancel(&self) { self.calls.lock().unwrap().push("cancel"); }
    }
    fn request(id: u64) -> PrepareRecording {
        PrepareRecording {
            recording_id: RecordingId::new(id),
            configuration: RecordingConfiguration::new(RecordingMode::Screen, false).unwrap(),
        }
    }
    #[test]
    fn aggregate_enforces_lifecycle_and_publishes_only_after_finalize() {
        let mut app = RecordingApplication::new(FakeResources::default());
        assert_eq!(app.prepare(request(1)).unwrap(), RecordingState::Ready);
        assert_eq!(app.start(RecordingId::new(1)).unwrap(), RecordingState::Recording);
        assert_eq!(app.pause(RecordingId::new(1)).unwrap(), RecordingState::Paused);
        assert_eq!(app.resume(RecordingId::new(1)).unwrap(), RecordingState::Recording);
        assert_eq!(app.stop(RecordingId::new(1)).unwrap(), RecordingState::Completed);
        assert!(app.session(RecordingId::new(1)).unwrap().artifact().is_some());
    }
    #[test]
    fn no_successful_artifact_on_finalization_failure() {
        let resources = FakeResources { fail_finalize: true, ..FakeResources::default() };
        let mut app = RecordingApplication::new(resources);
        app.prepare(request(2)).unwrap();
        app.start(RecordingId::new(2)).unwrap();
        assert!(app.stop(RecordingId::new(2)).is_err());
        let session = app.session(RecordingId::new(2)).unwrap();
        assert_eq!(session.state(), RecordingState::Failed);
        assert!(session.artifact().is_none());
    }
    #[test]
    fn cancel_is_allowed_only_before_capture_starts() {
        let mut app = RecordingApplication::new(FakeResources::default());
        app.prepare(request(3)).unwrap();
        assert_eq!(app.cancel(RecordingId::new(3)).unwrap(), RecordingState::Cancelled);
        assert!(app.start(RecordingId::new(3)).is_err());
    }
    #[test]
    fn duplicate_session_id_is_rejected() {
        let mut app = RecordingApplication::new(FakeResources::default());
        app.prepare(request(4)).unwrap();
        assert_eq!(app.prepare(request(4)), Err(ApplicationError::RecordingAlreadyExists(RecordingId::new(4))));
    }
    #[test]
    fn microphone_only_cannot_add_optional_microphone() {
        assert_eq!(
            RecordingConfiguration::new(RecordingMode::Microphone, true),
            Err(DomainError::MicrophoneAlreadyIsRecordingSource)
        );
    }
}
