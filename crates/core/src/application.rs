#![forbid(unsafe_code)]
//! Recording use cases: the inbound port and the application service that
//! coordinates the [`RecordingSession`] aggregate through the granular
//! outbound ports in [`crate::ports`].
//!
//! The service owns the *control plane*: which resource is opened, started,
//! drained, closed, published, or aborted, in which order, and what happens
//! when one of them fails. Media payloads never pass through it: they travel
//! on the bounded sinks the media pipeline creates at prepare and the service
//! hands to each capture adapter at start (ADR-0020). The service only keeps a
//! [`StreamMonitor`] per started source to observe source loss.

use std::fmt;

use framelet_capture_api::SourceId;

use crate::data_plane::{MediaSinks, StreamMonitor};
use crate::domain::{
    DomainError, RecordingConfiguration, RecordingId, RecordingSession, RecordingState, SourceKind,
    SourceLossOutcome,
};
use crate::ports::{
    CameraCapturePort, CaptureSessionId, ClosedOutput, MediaPipelinePort, MediaSessionId,
    MicrophoneCapturePort, OutputTarget, PortError, RecordingEvent, RecordingEventPublisher,
    RecordingStoragePort, ScreenCapturePort, SelectedSource, StorageSessionId,
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
    /// A source the recording mode cannot do without was lost; the recording
    /// stopped without publishing and the temporary output was kept.
    SourceLost {
        /// Kind of the lost source.
        kind: SourceKind,
        /// Failures observed while stopping the remaining resources.
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
            Self::SourceLost {
                kind,
                additional_failures,
            } => {
                write!(f, "recording stopped: {kind:?} source lost")?;
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
    ///
    /// Idempotent: on a session that already ended (completed, failed, or
    /// cancelled) it returns the terminal state and touches no port.
    fn stop(&mut self, recording_id: RecordingId) -> Result<RecordingState, ApplicationError>;
    /// Applies every source loss reported since the last call (ADR-0020).
    ///
    /// The inbound adapter's recording worker calls this periodically while a
    /// session records. For each lost source it publishes
    /// [`RecordingEvent::SourceLost`] and applies the mode-dependent
    /// [`SourceLossOutcome`]: continue without the optional microphone, or
    /// stop without publishing. Outside capture it returns the state unchanged.
    fn process_source_losses(
        &mut self,
        recording_id: RecordingId,
    ) -> Result<RecordingState, ApplicationError>;
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
        sinks: &mut MediaSinks,
    ) -> Result<(CaptureSessionId, StreamMonitor), CapabilityFailure> {
        let capability = RecordingCapability::for_source(kind);
        let missing_sink = || CapabilityFailure {
            capability: RecordingCapability::MediaPipeline,
            error: PortError::InvalidInput(format!("no media sink for {kind:?}")),
        };
        let unavailable = || CapabilityFailure {
            capability,
            error: PortError::SourceUnavailable,
        };
        let started = if kind == SourceKind::Microphone {
            let sink = sinks.take_audio().ok_or_else(missing_sink)?;
            let monitor = sink.monitor();
            let port = self.microphone.as_mut().ok_or_else(unavailable)?;
            port.start(source, sink).map(|session| (session, monitor))
        } else {
            let sink = sinks.take_video(kind).ok_or_else(missing_sink)?;
            let monitor = sink.monitor();
            let result = match kind {
                SourceKind::Screen => self.screen.as_mut().map(|port| port.start(source, sink)),
                _ => self.camera.as_mut().map(|port| port.start(source, sink)),
            };
            result
                .ok_or_else(unavailable)?
                .map(|session| (session, monitor))
        };
        started.map_err(|error| CapabilityFailure { capability, error })
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

/// A capture source that was started, with the monitor of the sink it feeds.
struct StartedCapture {
    kind: SourceKind,
    source: SourceId,
    session: CaptureSessionId,
    monitor: StreamMonitor,
}

/// Result of recording the source losses found in one check.
#[derive(Default)]
struct RecordedLosses {
    /// Losses recorded on the aggregate and published.
    recorded: Vec<StartedCapture>,
    /// Losses the aggregate rejected; still tracked by the caller.
    rejected: Vec<StartedCapture>,
    /// First recorded loss whose outcome is to stop the recording.
    stop_for: Option<SourceKind>,
    /// First rejection, reported once every loss has been processed.
    error: Option<ApplicationError>,
}

/// External resources held by one session. Not part of the aggregate: these
/// are adapter handles, not domain state.
struct SessionResources {
    sources: RecordingSources,
    storage: StorageSessionId,
    target: OutputTarget,
    media: MediaSessionId,
    /// Sinks not yet handed to a capture adapter; emptied by `start`.
    sinks: MediaSinks,
    captures: Vec<StartedCapture>,
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

    /// Releases a prepared-but-not-capturing session: abort media, abort storage.
    fn release_prepared(
        ports: &mut RecordingPorts,
        media: &MediaSessionId,
        storage: &StorageSessionId,
    ) -> Vec<CapabilityFailure> {
        let mut failures = Vec::new();
        if let Err(error) = ports.media.abort(media) {
            failures.push(media_failure(error));
        }
        if let Err(error) = ports.storage.abort(storage) {
            failures.push(storage_failure(error));
        }
        failures
    }

    /// Stops every started capture source, collecting failures and returning
    /// the stopped captures so their monitors can be inspected afterwards.
    fn stop_captures(
        ports: &mut RecordingPorts,
        resources: &mut SessionResources,
        failures: &mut Vec<CapabilityFailure>,
    ) -> Vec<StartedCapture> {
        let stopped = std::mem::take(&mut resources.captures);
        failures.extend(
            stopped
                .iter()
                .filter_map(|capture| ports.stop_capture(capture.kind, &capture.session).err()),
        );
        stopped
    }

    /// Drains and closes the pipeline, collecting failures.
    fn finalize_media(
        ports: &mut RecordingPorts,
        media: &MediaSessionId,
        failures: &mut Vec<CapabilityFailure>,
    ) -> Option<ClosedOutput> {
        if let Err(error) = ports.media.drain(media) {
            failures.push(media_failure(error));
        }
        match ports.media.close(media) {
            Ok(closed) => Some(closed),
            Err(error) => {
                failures.push(media_failure(error));
                None
            }
        }
    }

    /// Records every lost capture on the aggregate and publishes the fact.
    ///
    /// A loss the aggregate rejects does not stop the loop: every other loss
    /// is still recorded and published, and the rejected captures are handed
    /// back so the caller keeps tracking them instead of forgetting them.
    fn record_losses(&mut self, index: usize, lost: Vec<StartedCapture>) -> RecordedLosses {
        let mut losses = RecordedLosses::default();
        for capture in lost {
            let managed = &mut self.sessions[index];
            match managed.session.record_source_loss(capture.kind) {
                Ok(outcome) => {
                    self.ports.events.publish(RecordingEvent::SourceLost {
                        recording_id: managed.session.id(),
                        kind: capture.kind,
                        source: capture.source.clone(),
                    });
                    if outcome == SourceLossOutcome::StopRecording && losses.stop_for.is_none() {
                        losses.stop_for = Some(capture.kind);
                    }
                    losses.recorded.push(capture);
                }
                Err(error) => {
                    losses.error.get_or_insert(error.into());
                    losses.rejected.push(capture);
                }
            }
        }
        losses
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

        let reservation = self
            .ports
            .storage
            .begin(recording_id, session.configuration())
            .map_err(|error| {
                ApplicationError::capability_failed(storage_failure(error), Vec::new())
            })?;
        let storage_abort = |ports: &mut RecordingPorts| {
            ports
                .storage
                .abort(&reservation.session)
                .err()
                .map(storage_failure)
        };
        let prepared = match self.ports.media.prepare(
            recording_id,
            session.configuration(),
            &reservation.target,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                let additional_failures = storage_abort(&mut self.ports).into_iter().collect();
                return Err(ApplicationError::capability_failed(
                    media_failure(error),
                    additional_failures,
                ));
            }
        };
        if !prepared.sinks.matches(session.configuration()) {
            let additional_failures =
                Self::release_prepared(&mut self.ports, &prepared.session, &reservation.session);
            return Err(ApplicationError::capability_failed(
                media_failure(PortError::InvalidInput(
                    "media sinks do not match the recording configuration".into(),
                )),
                additional_failures,
            ));
        }

        session.mark_ready()?;
        let state = session.state();
        self.sessions.push(ManagedSession {
            session,
            resources: Some(SessionResources {
                sources,
                storage: reservation.session,
                target: reservation.target,
                media: prepared.session,
                sinks: prepared.sinks,
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
            failures.extend(Self::release_prepared(
                &mut self.ports,
                &resources.media,
                &resources.storage,
            ));
            return Err(self.fail_with(index, failures));
        }

        let configuration = self.sessions[index].session.configuration().clone();
        for kind in SOURCE_ORDER {
            if !configuration.requires_source(kind) {
                continue;
            }
            let started = match resources.sources.get(kind) {
                Some(source) => self
                    .ports
                    .start_capture(kind, source, &mut resources.sinks)
                    .map(|(session, monitor)| StartedCapture {
                        kind,
                        source: source.id.clone(),
                        session,
                        monitor,
                    }),
                None => Err(CapabilityFailure {
                    capability: RecordingCapability::for_source(kind),
                    error: PortError::SourceUnavailable,
                }),
            };
            match started {
                Ok(capture) => resources.captures.push(capture),
                Err(failure) => {
                    let mut failures = vec![failure];
                    Self::stop_captures(&mut self.ports, &mut resources, &mut failures);
                    failures.extend(Self::release_prepared(
                        &mut self.ports,
                        &resources.media,
                        &resources.storage,
                    ));
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
    /// media is flushed as far as possible. On any failure, or if a source the
    /// mode needs was lost, the session becomes `Failed`, nothing is
    /// published, and the temporary output is *kept* for recovery discovery
    /// instead of being aborted. Stopping an ended session is a no-op.
    fn stop(&mut self, id: RecordingId) -> Result<RecordingState, ApplicationError> {
        let index = self.index_of(id)?;
        let current = self.sessions[index].session.state();
        if matches!(
            current,
            RecordingState::Completed | RecordingState::Failed | RecordingState::Cancelled
        ) {
            return Ok(current);
        }
        let index = self.require_state(
            id,
            &[RecordingState::Recording, RecordingState::Paused],
            RecordingState::Recording,
        )?;
        self.sessions[index].session.begin_finalization()?;
        let Some(mut resources) = self.sessions[index].resources.take() else {
            return Err(self.fail_session(index, ApplicationError::ResourcesMissing(id)));
        };

        let mut failures = Vec::new();
        let stopped = Self::stop_captures(&mut self.ports, &mut resources, &mut failures);
        // Producers are stopped, so every monitor now shows its final state.
        let lost = stopped
            .into_iter()
            .filter(|capture| capture.monitor.source_lost())
            .collect();
        let losses = self.record_losses(index, lost);
        let closed = Self::finalize_media(&mut self.ports, &resources.media, &mut failures);
        if let Some(kind) = losses.stop_for {
            let error = ApplicationError::SourceLost {
                kind,
                additional_failures: failures,
            };
            return Err(self.fail_session(index, error));
        }
        if let Some(error) = losses.error {
            return Err(self.fail_session(index, error));
        }
        if !failures.is_empty() {
            return Err(self.fail_with(index, failures));
        }
        let Some(closed) = closed.filter(|closed| closed.target() == &resources.target) else {
            return Err(self.fail_with(
                index,
                vec![media_failure(PortError::InvalidInput(
                    "closed output does not match the reserved target".into(),
                ))],
            ));
        };

        match self.ports.storage.publish(&resources.storage, closed) {
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

    fn process_source_losses(
        &mut self,
        id: RecordingId,
    ) -> Result<RecordingState, ApplicationError> {
        let index = self.index_of(id)?;
        let state = self.sessions[index].session.state();
        if !matches!(state, RecordingState::Recording | RecordingState::Paused) {
            return Ok(state);
        }
        let Some(mut resources) = self.sessions[index].resources.take() else {
            return Err(self.fail_session(index, ApplicationError::ResourcesMissing(id)));
        };
        let (lost, live): (Vec<_>, Vec<_>) = resources
            .captures
            .drain(..)
            .partition(|capture| capture.monitor.source_lost());
        resources.captures = live;
        if lost.is_empty() {
            self.sessions[index].resources = Some(resources);
            return Ok(state);
        }

        let losses = self.record_losses(index, lost);
        // A loss the aggregate rejected stays tracked, so later checks see it again.
        resources.captures.extend(losses.rejected);
        let mut failures: Vec<CapabilityFailure> = losses
            .recorded
            .iter()
            .filter_map(|capture| {
                self.ports
                    .stop_capture(capture.kind, &capture.session)
                    .err()
            })
            .collect();
        let Some(kind) = losses.stop_for else {
            self.sessions[index].resources = Some(resources);
            return match (losses.error, failures.is_empty()) {
                (Some(error), _) => Err(error),
                (None, true) => Ok(state),
                (None, false) => Err(ApplicationError::CleanupIncomplete { state, failures }),
            };
        };

        Self::stop_captures(&mut self.ports, &mut resources, &mut failures);
        // Finalize as far as possible so the kept candidate is recoverable,
        // but never publish: the configured recording is incomplete.
        let _closed = Self::finalize_media(&mut self.ports, &resources.media, &mut failures);
        let error = ApplicationError::SourceLost {
            kind,
            additional_failures: failures,
        };
        Err(self.fail_session(index, error))
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
            Some(resources) => {
                Self::release_prepared(&mut self.ports, &resources.media, &resources.storage)
            }
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
