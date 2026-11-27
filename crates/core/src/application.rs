#![forbid(unsafe_code)]
//! Recording use cases: the inbound port and the application service that
//! coordinates the [`RecordingSession`] aggregate through the granular
//! outbound ports in [`crate::ports`].
//!
//! The service owns the *control plane*: which resource is opened, started,
//! drained, closed, published, or aborted, in which order, and what happens
//! when one of them fails. It does not route media payloads; that data-plane
//! contract is a separate roadmap item.

use std::fmt;

use crate::domain::{
    DomainError, RecordingConfiguration, RecordingId, RecordingSession, RecordingState, SourceKind,
};
use crate::ports::{
    CameraCapturePort, CaptureSessionId, MediaPipelinePort, MediaSessionId, MicrophoneCapturePort,
    PortError, RecordingEvent, RecordingEventPublisher, RecordingStoragePort, ScreenCapturePort,
    SelectedSource,
};

/// Source kinds in the order in which capture is started and stopped.
const SOURCE_ORDER: [SourceKind; 3] = [
    SourceKind::Screen,
    SourceKind::Camera,
    SourceKind::Microphone,
];

/// Sources the user selected for a recording, one optional entry per kind.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecordingSources {
    /// Selected screen or window.
    pub screen: Option<SelectedSource>,
    /// Selected camera.
    pub camera: Option<SelectedSource>,
    /// Selected microphone.
    pub microphone: Option<SelectedSource>,
}

impl RecordingSources {
    /// Returns the selected source of the given kind, if any.
    #[must_use]
    pub const fn get(&self, kind: SourceKind) -> Option<&SelectedSource> {
        match kind {
            SourceKind::Screen => self.screen.as_ref(),
            SourceKind::Camera => self.camera.as_ref(),
            SourceKind::Microphone => self.microphone.as_ref(),
        }
    }
}

/// Request accepted by the prepare-recording inbound port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrepareRecording {
    /// Stable identity for the new session.
    pub recording_id: RecordingId,
    /// Validated recording configuration.
    pub configuration: RecordingConfiguration,
    /// Selected sources; must match exactly what the configuration requires.
    pub sources: RecordingSources,
}

/// Outbound capability involved in a failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingCapability {
    /// Temporary output reservation, publication, and abort.
    Storage,
    /// Encoding, muxing, drain, and close.
    MediaPipeline,
    /// Screen or window capture.
    ScreenCapture,
    /// Camera capture.
    CameraCapture,
    /// Microphone capture.
    MicrophoneCapture,
}

impl RecordingCapability {
    /// Returns the capture capability that serves a source kind.
    #[must_use]
    pub const fn for_source(kind: SourceKind) -> Self {
        match kind {
            SourceKind::Screen => Self::ScreenCapture,
            SourceKind::Camera => Self::CameraCapture,
            SourceKind::Microphone => Self::MicrophoneCapture,
        }
    }
}

/// A typed port failure attributed to the capability that produced it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityFailure {
    /// Capability whose port returned the error.
    pub capability: RecordingCapability,
    /// Typed error returned by the port.
    pub error: PortError,
}

impl fmt::Display for CapabilityFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.capability, self.error)
    }
}

/// Typed application-level failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationError {
    /// Domain rejected the requested operation.
    Domain(DomainError),
    /// The configuration needs a capability that this build or platform does not provide.
    CapabilityUnavailable(RecordingCapability),
    /// A required outbound capability failed.
    CapabilityFailed {
        /// First failure, which decided the outcome.
        failure: CapabilityFailure,
        /// Further failures observed while stopping or releasing resources.
        additional_failures: Vec<CapabilityFailure>,
    },
    /// The requested operation took effect, but releasing resources reported failures.
    CleanupIncomplete {
        /// Lifecycle state after the operation.
        state: RecordingState,
        /// Failures observed while releasing resources.
        failures: Vec<CapabilityFailure>,
    },
    /// An active session has no resource handles; an application invariant was violated.
    ResourcesMissing(RecordingId),
    /// A session identity is unknown to the application service.
    RecordingNotFound(RecordingId),
    /// A session already exists for the requested identity.
    RecordingAlreadyExists(RecordingId),
}

impl ApplicationError {
    /// Builds a failure from the first decisive failure and any further ones.
    fn capability_failed(
        failure: CapabilityFailure,
        additional_failures: Vec<CapabilityFailure>,
    ) -> Self {
        Self::CapabilityFailed {
            failure,
            additional_failures,
        }
    }

    /// Builds a failure from a non-empty list; returns `None` for an empty list.
    fn from_failures(mut failures: Vec<CapabilityFailure>) -> Option<Self> {
        if failures.is_empty() {
            return None;
        }
        let failure = failures.remove(0);
        Some(Self::capability_failed(failure, failures))
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(f, "{error}"),
            Self::CapabilityUnavailable(capability) => {
                write!(f, "recording capability {capability:?} is not available")
            }
            Self::CapabilityFailed {
                failure,
                additional_failures,
            } => {
                write!(f, "recording capability failed: {failure}")?;
                if !additional_failures.is_empty() {
                    write!(f, " (+{} further failure(s))", additional_failures.len())?;
                }
                Ok(())
            }
            Self::CleanupIncomplete { state, failures } => write!(
                f,
                "recording reached {state:?}, but {} resource(s) could not be released",
                failures.len()
            ),
            Self::ResourcesMissing(id) => {
                write!(f, "recording {} has no prepared resources", id.value())
            }
            Self::RecordingNotFound(id) => write!(f, "recording {} was not found", id.value()),
            Self::RecordingAlreadyExists(id) => {
                write!(f, "recording {} already exists", id.value())
            }
        }
    }
}

impl std::error::Error for ApplicationError {}

impl From<DomainError> for ApplicationError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

/// Inbound port implemented by the recording application service.
pub trait RecordingUseCases: Send {
    /// Validates sources, reserves storage, and prepares the media pipeline.
    fn prepare(&mut self, request: PrepareRecording) -> Result<RecordingState, ApplicationError>;
    /// Starts the media pipeline and every selected capture source.
    fn start(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Pauses an active recording.
    fn pause(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Resumes a paused recording.
    fn resume(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Stops capture, drains and closes the pipeline, then publishes the output.
    fn stop(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Cancels preparation or a ready session and releases its resources.
    fn cancel(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Queries the current lifecycle state.
    fn state(&self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
}

/// Outbound ports injected by the composition root.
///
/// Capture ports are optional so that a build can offer only the capabilities
/// that are actually implemented; a configuration that needs a missing one is
/// rejected with [`ApplicationError::CapabilityUnavailable`] before any
/// resource is touched.
pub struct RecordingPorts {
    /// Transactional output storage.
    pub storage: Box<dyn RecordingStoragePort>,
    /// Media pipeline (encode, mux, finalize).
    pub media: Box<dyn MediaPipelinePort>,
    /// Screen capture, if available.
    pub screen: Option<Box<dyn ScreenCapturePort>>,
    /// Camera capture, if available.
    pub camera: Option<Box<dyn CameraCapturePort>>,
    /// Microphone capture, if available.
    pub microphone: Option<Box<dyn MicrophoneCapturePort>>,
    /// Lifecycle event publication.
    pub events: Box<dyn RecordingEventPublisher>,
}

impl RecordingPorts {
    const fn provides(&self, kind: SourceKind) -> bool {
        match kind {
            SourceKind::Screen => self.screen.is_some(),
            SourceKind::Camera => self.camera.is_some(),
            SourceKind::Microphone => self.microphone.is_some(),
        }
    }

    fn start_capture(
        &mut self,
        kind: SourceKind,
        source: &SelectedSource,
    ) -> Result<CaptureSessionId, CapabilityFailure> {
        let capability = RecordingCapability::for_source(kind);
        let result = match kind {
            SourceKind::Screen => self.screen.as_mut().map(|port| port.start(source)),
            SourceKind::Camera => self.camera.as_mut().map(|port| port.start(source)),
            SourceKind::Microphone => self.microphone.as_mut().map(|port| port.start(source)),
        };
        result
            .unwrap_or(Err(PortError::SourceUnavailable))
            .map_err(|error| CapabilityFailure { capability, error })
    }

    fn stop_capture(
        &mut self,
        kind: SourceKind,
        session: &CaptureSessionId,
    ) -> Result<(), CapabilityFailure> {
        let capability = RecordingCapability::for_source(kind);
        let result = match kind {
            SourceKind::Screen => self.screen.as_mut().map(|port| port.stop(session)),
            SourceKind::Camera => self.camera.as_mut().map(|port| port.stop(session)),
            SourceKind::Microphone => self.microphone.as_mut().map(|port| port.stop(session)),
        };
        result
            .unwrap_or(Err(PortError::SourceUnavailable))
            .map_err(|error| CapabilityFailure { capability, error })
    }
}

fn storage_failure(error: PortError) -> CapabilityFailure {
    CapabilityFailure {
        capability: RecordingCapability::Storage,
        error,
    }
}

fn media_failure(error: PortError) -> CapabilityFailure {
    CapabilityFailure {
        capability: RecordingCapability::MediaPipeline,
        error,
    }
}

/// External resources held by one session. Not part of the aggregate: these
/// are adapter handles, not domain state.
struct SessionResources {
    sources: RecordingSources,
    storage: MediaSessionId,
    media: MediaSessionId,
    captures: Vec<(SourceKind, CaptureSessionId)>,
}

struct ManagedSession {
    session: RecordingSession,
    resources: Option<SessionResources>,
}

/// Application service coordinating the Recording aggregate through outbound ports.
pub struct RecordingApplication {
    ports: RecordingPorts,
    sessions: Vec<ManagedSession>,
}

impl RecordingApplication {
    /// Creates an application service with injected outbound ports.
    #[must_use]
    pub const fn new(ports: RecordingPorts) -> Self {
        Self {
            ports,
            sessions: Vec::new(),
        }
    }

    /// Returns the aggregate for a session, if it exists.
    #[must_use]
    pub fn session(&self, id: RecordingId) -> Option<&RecordingSession> {
        self.sessions
            .iter()
            .find(|managed| managed.session.id() == id)
            .map(|managed| &managed.session)
    }

    fn index_of(&self, id: RecordingId) -> Result<usize, ApplicationError> {
        self.sessions
            .iter()
            .position(|managed| managed.session.id() == id)
            .ok_or(ApplicationError::RecordingNotFound(id))
    }

    fn require_state(
        &self,
        id: RecordingId,
        allowed: &[RecordingState],
        expected: RecordingState,
    ) -> Result<usize, ApplicationError> {
        let index = self.index_of(id)?;
        let actual = self.sessions[index].session.state();
        if allowed.contains(&actual) {
            Ok(index)
        } else {
            Err(DomainError::UnexpectedState { expected, actual }.into())
        }
    }

    /// Releases a prepared-but-not-capturing session: close media, abort storage.
    fn release_prepared(
        ports: &mut RecordingPorts,
        resources: &SessionResources,
    ) -> Vec<CapabilityFailure> {
        let mut failures = Vec::new();
        if let Err(error) = ports.media.close(&resources.media) {
            failures.push(media_failure(error));
        }
        if let Err(error) = ports.storage.abort(&resources.storage) {
            failures.push(storage_failure(error));
        }
        failures
    }

    /// Stops every started capture source, collecting failures.
    fn stop_captures(
        ports: &mut RecordingPorts,
        resources: &mut SessionResources,
    ) -> Vec<CapabilityFailure> {
        resources
            .captures
            .drain(..)
            .filter_map(|(kind, session)| ports.stop_capture(kind, &session).err())
            .collect()
    }

    /// Marks a session failed, publishes the fact, and returns the error.
    fn fail_session(&mut self, index: usize, error: ApplicationError) -> ApplicationError {
        let managed = &mut self.sessions[index];
        managed.resources = None;
        // `fail` only rejects Completed/Cancelled; callers only pass active sessions.
        if let Err(domain) = managed.session.fail() {
            return domain.into();
        }
        self.ports.events.publish(RecordingEvent::Failed {
            recording_id: managed.session.id(),
            reason: error.to_string(),
        });
        error
    }

    /// Like [`Self::fail_session`] for a list of capability failures.
    fn fail_with(&mut self, index: usize, failures: Vec<CapabilityFailure>) -> ApplicationError {
        let id = self.sessions[index].session.id();
        let error = ApplicationError::from_failures(failures)
            .unwrap_or(ApplicationError::ResourcesMissing(id));
        self.fail_session(index, error)
    }
}

impl RecordingUseCases for RecordingApplication {
    fn prepare(&mut self, request: PrepareRecording) -> Result<RecordingState, ApplicationError> {
        let PrepareRecording {
            recording_id,
            configuration,
            sources,
        } = request;
        if self.index_of(recording_id).is_ok() {
            return Err(ApplicationError::RecordingAlreadyExists(recording_id));
        }
        configuration.validate_source_selection(|kind| sources.get(kind).is_some())?;
        if let Some(kind) = SOURCE_ORDER
            .into_iter()
            .find(|kind| configuration.requires_source(*kind) && !self.ports.provides(*kind))
        {
            return Err(ApplicationError::CapabilityUnavailable(
                RecordingCapability::for_source(kind),
            ));
        }

        let mut session = RecordingSession::new(recording_id, configuration);
        session.begin_preparation()?;

        let storage = self
            .ports
            .storage
            .begin(recording_id, session.configuration())
            .map_err(|error| {
                ApplicationError::capability_failed(storage_failure(error), Vec::new())
            })?;
        let media = match self
            .ports
            .media
            .prepare(recording_id, session.configuration().mode())
        {
            Ok(media) => media,
            Err(error) => {
                let additional_failures = self
                    .ports
                    .storage
                    .abort(&storage)
                    .err()
                    .map(storage_failure)
                    .into_iter()
                    .collect();
                return Err(ApplicationError::capability_failed(
                    media_failure(error),
                    additional_failures,
                ));
            }
        };

        session.mark_ready()?;
        let state = session.state();
        self.sessions.push(ManagedSession {
            session,
            resources: Some(SessionResources {
                sources,
                storage,
                media,
                captures: Vec::new(),
            }),
        });
        self.ports
            .events
            .publish(RecordingEvent::Ready { recording_id });
        Ok(state)
    }

    fn start(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        let index = self.require_state(id, &[RecordingState::Ready], RecordingState::Ready)?;
        let Some(mut resources) = self.sessions[index].resources.take() else {
            return Err(self.fail_session(index, ApplicationError::ResourcesMissing(id)));
        };

        if let Err(error) = self.ports.media.start(&resources.media) {
            let mut failures = vec![media_failure(error)];
            failures.extend(Self::release_prepared(&mut self.ports, &resources));
            return Err(self.fail_with(index, failures));
        }

        let configuration = self.sessions[index].session.configuration().clone();
        for kind in SOURCE_ORDER {
            if !configuration.requires_source(kind) {
                continue;
            }
            let started = match resources.sources.get(kind) {
                Some(source) => self.ports.start_capture(kind, source),
                None => Err(CapabilityFailure {
                    capability: RecordingCapability::for_source(kind),
                    error: PortError::SourceUnavailable,
                }),
            };
            match started {
                Ok(capture) => resources.captures.push((kind, capture)),
                Err(failure) => {
                    let mut failures = vec![failure];
                    failures.extend(Self::stop_captures(&mut self.ports, &mut resources));
                    failures.extend(Self::release_prepared(&mut self.ports, &resources));
                    return Err(self.fail_with(index, failures));
                }
            }
        }

        let managed = &mut self.sessions[index];
        managed.session.start()?;
        managed.resources = Some(resources);
        self.ports
            .events
            .publish(RecordingEvent::Started { recording_id: id });
        Ok(RecordingState::Recording)
    }

    fn pause(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        let index =
            self.require_state(id, &[RecordingState::Recording], RecordingState::Recording)?;
        let Some(resources) = &self.sessions[index].resources else {
            return Err(ApplicationError::ResourcesMissing(id));
        };
        self.ports.media.pause(&resources.media).map_err(|error| {
            ApplicationError::capability_failed(media_failure(error), Vec::new())
        })?;
        self.sessions[index].session.pause()?;
        self.ports
            .events
            .publish(RecordingEvent::Paused { recording_id: id });
        Ok(RecordingState::Paused)
    }

    fn resume(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        let index = self.require_state(id, &[RecordingState::Paused], RecordingState::Paused)?;
        let Some(resources) = &self.sessions[index].resources else {
            return Err(ApplicationError::ResourcesMissing(id));
        };
        self.ports.media.resume(&resources.media).map_err(|error| {
            ApplicationError::capability_failed(media_failure(error), Vec::new())
        })?;
        self.sessions[index].session.resume()?;
        self.ports
            .events
            .publish(RecordingEvent::Resumed { recording_id: id });
        Ok(RecordingState::Recording)
    }

    /// Stops capture, drains and closes the pipeline, then publishes the output.
    ///
    /// Every step is attempted even if an earlier one failed, so accepted
    /// media is flushed as far as possible. On any failure the session becomes
    /// `Failed`, nothing is published, and the temporary output is *kept* for
    /// recovery discovery instead of being aborted.
    fn stop(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        let index = self.require_state(
            id,
            &[RecordingState::Recording, RecordingState::Paused],
            RecordingState::Recording,
        )?;
        self.sessions[index].session.begin_finalization()?;
        let Some(mut resources) = self.sessions[index].resources.take() else {
            return Err(self.fail_session(index, ApplicationError::ResourcesMissing(id)));
        };

        let mut failures = Self::stop_captures(&mut self.ports, &mut resources);
        if let Err(error) = self.ports.media.drain(&resources.media) {
            failures.push(media_failure(error));
        }
        if let Err(error) = self.ports.media.close(&resources.media) {
            failures.push(media_failure(error));
        }
        if !failures.is_empty() {
            return Err(self.fail_with(index, failures));
        }

        match self.ports.storage.publish(&resources.storage) {
            Ok(artifact) => {
                self.sessions[index].session.complete(artifact.clone())?;
                self.ports.events.publish(RecordingEvent::Completed {
                    recording_id: id,
                    artifact,
                });
                Ok(RecordingState::Completed)
            }
            Err(error) => Err(self.fail_with(index, vec![storage_failure(error)])),
        }
    }

    fn cancel(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        let index = self.require_state(
            id,
            &[
                RecordingState::Idle,
                RecordingState::Preparing,
                RecordingState::Ready,
            ],
            RecordingState::Ready,
        )?;
        let failures = match self.sessions[index].resources.take() {
            Some(resources) => Self::release_prepared(&mut self.ports, &resources),
            None => Vec::new(),
        };
        self.sessions[index].session.cancel()?;
        self.ports
            .events
            .publish(RecordingEvent::Cancelled { recording_id: id });
        if failures.is_empty() {
            Ok(RecordingState::Cancelled)
        } else {
            Err(ApplicationError::CleanupIncomplete {
                state: RecordingState::Cancelled,
                failures,
            })
        }
    }

    fn state(&self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        let index = self.index_of(id)?;
        Ok(self.sessions[index].session.state())
    }
}

#[cfg(test)]
#[path = "application_tests.rs"]
mod tests;
