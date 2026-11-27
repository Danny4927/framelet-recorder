#![forbid(unsafe_code)]
//! Recording bounded-context domain model.
//!
//! The aggregate owns lifecycle invariants. Nothing in this module knows about
//! adapters, ports, threads, or external resources.

use std::fmt;

pub use framelet_capture_api::SourceKind;

/// Stable identity of a recording session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecordingId(u64);

impl RecordingId {
    /// Creates an identifier from a caller-provided unique value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    /// Returns the underlying identifier.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
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
        Ok(Self {
            mode,
            include_microphone,
        })
    }
    /// Returns the selected recording mode.
    #[must_use]
    pub const fn mode(&self) -> RecordingMode {
        self.mode
    }
    /// Returns whether a microphone is an optional additional source.
    #[must_use]
    pub const fn includes_microphone(&self) -> bool {
        self.include_microphone
    }
    /// Returns whether this configuration needs a source of the given kind.
    ///
    /// Screen and camera follow from the mode; a microphone is required in
    /// microphone-only mode and when optional microphone input is enabled.
    #[must_use]
    pub const fn requires_source(&self, kind: SourceKind) -> bool {
        match kind {
            SourceKind::Screen => matches!(
                self.mode,
                RecordingMode::Screen | RecordingMode::ScreenAndWebcam
            ),
            SourceKind::Camera => matches!(
                self.mode,
                RecordingMode::Webcam | RecordingMode::ScreenAndWebcam
            ),
            SourceKind::Microphone => {
                matches!(self.mode, RecordingMode::Microphone) || self.include_microphone
            }
        }
    }
    /// Decides what losing a source of `kind` during capture means for the recording.
    ///
    /// Losing a source the mode itself records (screen, camera, or the
    /// microphone in microphone-only mode) makes the configured recording
    /// impossible, so it must stop. Losing the *optional* microphone leaves the
    /// mode valid: capture continues, and the loss is recorded on the session.
    #[must_use]
    pub const fn source_loss_outcome(&self, kind: SourceKind) -> SourceLossOutcome {
        match (kind, self.mode) {
            (SourceKind::Microphone, mode) if !matches!(mode, RecordingMode::Microphone) => {
                SourceLossOutcome::ContinueWithout
            }
            _ => SourceLossOutcome::StopRecording,
        }
    }
    /// Checks that exactly the required source kinds were selected.
    ///
    /// `selected` reports, for each kind, whether the user selected a source of it.
    pub fn validate_source_selection(
        &self,
        selected: impl Fn(SourceKind) -> bool,
    ) -> Result<(), DomainError> {
        for kind in [
            SourceKind::Screen,
            SourceKind::Camera,
            SourceKind::Microphone,
        ] {
            match (self.requires_source(kind), selected(kind)) {
                (true, false) => return Err(DomainError::MissingSource(kind)),
                (false, true) => return Err(DomainError::UnexpectedSource(kind)),
                _ => {}
            }
        }
        Ok(())
    }
}

/// Consequence of losing a capture source while recording.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceLossOutcome {
    /// The mode stays valid without the source; capture continues degraded.
    ContinueWithout,
    /// The mode cannot be recorded without the source; the recording stops
    /// and must not be reported as complete.
    StopRecording,
}

/// A finalized recording output descriptor owned by the application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingArtifact {
    location: String,
}

impl RecordingArtifact {
    /// Creates an artifact descriptor from a non-empty adapter-neutral location.
    pub fn new(location: String) -> Result<Self, DomainError> {
        if location.trim().is_empty() {
            return Err(DomainError::EmptyArtifactLocation);
        }
        Ok(Self { location })
    }
    /// Returns the adapter-neutral output location.
    #[must_use]
    pub fn location(&self) -> &str {
        &self.location
    }
}

/// Recording session aggregate; owns lifecycle invariants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingSession {
    id: RecordingId,
    configuration: RecordingConfiguration,
    state: RecordingState,
    artifact: Option<RecordingArtifact>,
    lost_sources: Vec<SourceKind>,
}

impl RecordingSession {
    /// Starts a new session in the idle state.
    #[must_use]
    pub const fn new(id: RecordingId, configuration: RecordingConfiguration) -> Self {
        Self {
            id,
            configuration,
            state: RecordingState::Idle,
            artifact: None,
            lost_sources: Vec::new(),
        }
    }
    /// Returns the session identity.
    #[must_use]
    pub const fn id(&self) -> RecordingId {
        self.id
    }
    /// Returns the immutable session configuration.
    #[must_use]
    pub const fn configuration(&self) -> &RecordingConfiguration {
        &self.configuration
    }
    /// Returns the current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> RecordingState {
        self.state
    }
    /// Returns the finalized artifact, if finalization succeeded.
    #[must_use]
    pub const fn artifact(&self) -> Option<&RecordingArtifact> {
        self.artifact.as_ref()
    }
    /// Returns the sources lost during capture, in the order they were lost.
    ///
    /// A completed session with lost sources is a degraded recording.
    #[must_use]
    pub fn lost_sources(&self) -> &[SourceKind] {
        &self.lost_sources
    }
    /// Records that a capture source was lost and returns what that means.
    ///
    /// Only sources the configuration records can be lost, and only while
    /// capturing or finalizing. Recording the same loss twice is a no-op.
    pub fn record_source_loss(
        &mut self,
        kind: SourceKind,
    ) -> Result<SourceLossOutcome, DomainError> {
        if !matches!(
            self.state,
            RecordingState::Recording | RecordingState::Paused | RecordingState::Finalizing
        ) {
            return Err(DomainError::UnexpectedState {
                expected: RecordingState::Recording,
                actual: self.state,
            });
        }
        if !self.configuration.requires_source(kind) {
            return Err(DomainError::UnexpectedSource(kind));
        }
        if !self.lost_sources.contains(&kind) {
            self.lost_sources.push(kind);
        }
        Ok(self.configuration.source_loss_outcome(kind))
    }
    /// Moves the session to preparing.
    pub fn begin_preparation(&mut self) -> Result<(), DomainError> {
        self.transition(RecordingState::Preparing)
    }
    /// Marks all required sources and output as ready.
    pub fn mark_ready(&mut self) -> Result<(), DomainError> {
        self.transition(RecordingState::Ready)
    }
    /// Starts media capture.
    pub fn start(&mut self) -> Result<(), DomainError> {
        self.transition(RecordingState::Recording)
    }
    /// Pauses media capture.
    pub fn pause(&mut self) -> Result<(), DomainError> {
        self.transition(RecordingState::Paused)
    }
    /// Resumes media capture.
    pub fn resume(&mut self) -> Result<(), DomainError> {
        self.transition(RecordingState::Recording)
    }
    /// Begins stopping producers and finalizing accepted media.
    pub fn begin_finalization(&mut self) -> Result<(), DomainError> {
        self.transition(RecordingState::Finalizing)
    }
    /// Marks finalization successful and publishes the artifact.
    pub fn complete(&mut self, artifact: RecordingArtifact) -> Result<(), DomainError> {
        self.require_state(RecordingState::Finalizing)?;
        self.artifact = Some(artifact);
        self.state = RecordingState::Completed;
        Ok(())
    }
    /// Marks the session failed. A failed session never exposes an artifact.
    pub fn fail(&mut self) -> Result<(), DomainError> {
        if matches!(
            self.state,
            RecordingState::Completed | RecordingState::Cancelled
        ) {
            return Err(DomainError::InvalidTransition {
                from: self.state,
                to: RecordingState::Failed,
            });
        }
        self.artifact = None;
        self.state = RecordingState::Failed;
        Ok(())
    }
    /// Cancels preparation or a ready session; active recordings must be stopped.
    pub fn cancel(&mut self) -> Result<(), DomainError> {
        if !matches!(
            self.state,
            RecordingState::Idle | RecordingState::Preparing | RecordingState::Ready
        ) {
            return Err(DomainError::InvalidTransition {
                from: self.state,
                to: RecordingState::Cancelled,
            });
        }
        self.state = RecordingState::Cancelled;
        Ok(())
    }
    /// Resets a terminal session for a new lifecycle attempt.
    pub fn reset(&mut self) -> Result<(), DomainError> {
        if !matches!(
            self.state,
            RecordingState::Completed | RecordingState::Failed | RecordingState::Cancelled
        ) {
            return Err(DomainError::InvalidTransition {
                from: self.state,
                to: RecordingState::Idle,
            });
        }
        self.artifact = None;
        self.lost_sources.clear();
        self.state = RecordingState::Idle;
        Ok(())
    }
    fn transition(&mut self, to: RecordingState) -> Result<(), DomainError> {
        if !is_allowed_transition(self.state, to) {
            return Err(DomainError::InvalidTransition {
                from: self.state,
                to,
            });
        }
        self.state = to;
        Ok(())
    }
    fn require_state(&self, expected: RecordingState) -> Result<(), DomainError> {
        if self.state == expected {
            Ok(())
        } else {
            Err(DomainError::UnexpectedState {
                expected,
                actual: self.state,
            })
        }
    }
}

/// Determines legal lifecycle transitions for a recording session.
#[must_use]
pub const fn is_allowed_transition(from: RecordingState, to: RecordingState) -> bool {
    use RecordingState::{
        Cancelled, Completed, Failed, Finalizing, Idle, Paused, Preparing, Ready, Recording,
    };
    matches!(
        (from, to),
        (Idle, Preparing)
            | (Preparing, Ready)
            | (Preparing, Failed)
            | (Preparing, Cancelled)
            | (Ready, Recording)
            | (Ready, Failed)
            | (Ready, Cancelled)
            | (Recording, Paused)
            | (Recording, Finalizing)
            | (Recording, Failed)
            | (Paused, Recording)
            | (Paused, Finalizing)
            | (Paused, Failed)
            | (Finalizing, Completed)
            | (Finalizing, Failed)
            | (Completed, Idle)
            | (Failed, Idle)
            | (Cancelled, Idle)
    )
}

/// Domain invariant or lifecycle violation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainError {
    /// Optional microphone selection duplicates the microphone-only mode.
    MicrophoneAlreadyIsRecordingSource,
    /// An output location must not be empty.
    EmptyArtifactLocation,
    /// The configuration requires a source kind that was not selected.
    MissingSource(SourceKind),
    /// A source kind was selected that the configuration does not use.
    UnexpectedSource(SourceKind),
    /// A lifecycle transition is not permitted.
    InvalidTransition {
        /// State the aggregate was in before the attempted transition.
        from: RecordingState,
        /// State requested by the rejected operation.
        to: RecordingState,
    },
    /// An operation requires a specific lifecycle state.
    UnexpectedState {
        /// State required by the attempted operation.
        expected: RecordingState,
        /// Actual state of the aggregate.
        actual: RecordingState,
    },
}
impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MicrophoneAlreadyIsRecordingSource => write!(
                f,
                "microphone-only mode cannot add a second microphone source"
            ),
            Self::EmptyArtifactLocation => {
                write!(f, "recording artifact location must not be empty")
            }
            Self::MissingSource(kind) => {
                write!(f, "recording configuration requires a {kind:?} source")
            }
            Self::UnexpectedSource(kind) => {
                write!(f, "recording configuration does not use a {kind:?} source")
            }
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid recording transition from {from:?} to {to:?}")
            }
            Self::UnexpectedState { expected, actual } => write!(
                f,
                "recording operation requires {expected:?}, current state is {actual:?}"
            ),
        }
    }
}
impl std::error::Error for DomainError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(mode: RecordingMode, microphone: bool) -> RecordingConfiguration {
        RecordingConfiguration::new(mode, microphone).unwrap()
    }

    #[test]
    fn microphone_only_cannot_add_optional_microphone() {
        assert_eq!(
            RecordingConfiguration::new(RecordingMode::Microphone, true),
            Err(DomainError::MicrophoneAlreadyIsRecordingSource)
        );
    }

    #[test]
    fn required_sources_follow_mode_and_optional_microphone() {
        use SourceKind::{Camera, Microphone, Screen};
        let cases = [
            (RecordingMode::Screen, false, [true, false, false]),
            (RecordingMode::Screen, true, [true, false, true]),
            (RecordingMode::Microphone, false, [false, false, true]),
            (RecordingMode::Webcam, false, [false, true, false]),
            (RecordingMode::Webcam, true, [false, true, true]),
            (RecordingMode::ScreenAndWebcam, false, [true, true, false]),
            (RecordingMode::ScreenAndWebcam, true, [true, true, true]),
        ];
        for (mode, microphone, expected) in cases {
            let configuration = config(mode, microphone);
            let actual =
                [Screen, Camera, Microphone].map(|kind| configuration.requires_source(kind));
            assert_eq!(actual, expected, "{mode:?} microphone={microphone}");
        }
    }

    #[test]
    fn source_selection_must_match_exactly() {
        let configuration = config(RecordingMode::Screen, true);
        assert_eq!(
            configuration.validate_source_selection(|kind| kind != SourceKind::Camera),
            Ok(())
        );
        assert_eq!(
            configuration.validate_source_selection(|kind| kind == SourceKind::Screen),
            Err(DomainError::MissingSource(SourceKind::Microphone))
        );
        assert_eq!(
            configuration.validate_source_selection(|_| true),
            Err(DomainError::UnexpectedSource(SourceKind::Camera))
        );
    }

    #[test]
    fn transition_table_is_exhaustively_enforced() {
        use RecordingState::{
            Cancelled, Completed, Failed, Finalizing, Idle, Paused, Preparing, Ready, Recording,
        };
        let all = [
            Idle, Preparing, Ready, Recording, Paused, Finalizing, Completed, Failed, Cancelled,
        ];
        let allowed = [
            (Idle, Preparing),
            (Preparing, Ready),
            (Preparing, Failed),
            (Preparing, Cancelled),
            (Ready, Recording),
            (Ready, Failed),
            (Ready, Cancelled),
            (Recording, Paused),
            (Recording, Finalizing),
            (Recording, Failed),
            (Paused, Recording),
            (Paused, Finalizing),
            (Paused, Failed),
            (Finalizing, Completed),
            (Finalizing, Failed),
            (Completed, Idle),
            (Failed, Idle),
            (Cancelled, Idle),
        ];
        for from in all {
            for to in all {
                assert_eq!(
                    is_allowed_transition(from, to),
                    allowed.contains(&(from, to)),
                    "{from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn artifact_is_exposed_only_after_completion_and_cleared_on_failure() {
        let mut session =
            RecordingSession::new(RecordingId::new(1), config(RecordingMode::Screen, false));
        let artifact = RecordingArtifact::new("out.mkv".into()).unwrap();
        assert!(session.complete(artifact.clone()).is_err());
        session.begin_preparation().unwrap();
        session.mark_ready().unwrap();
        session.start().unwrap();
        session.begin_finalization().unwrap();
        session.complete(artifact).unwrap();
        assert!(session.artifact().is_some());
        assert!(session.fail().is_err());
        assert!(session.cancel().is_err());
    }

    #[test]
    fn source_loss_outcome_depends_on_mode() {
        use SourceLossOutcome::{ContinueWithout, StopRecording};
        let cases = [
            (
                RecordingMode::Screen,
                true,
                SourceKind::Screen,
                StopRecording,
            ),
            (
                RecordingMode::Screen,
                true,
                SourceKind::Microphone,
                ContinueWithout,
            ),
            (
                RecordingMode::Microphone,
                false,
                SourceKind::Microphone,
                StopRecording,
            ),
            (
                RecordingMode::Webcam,
                true,
                SourceKind::Camera,
                StopRecording,
            ),
            (
                RecordingMode::Webcam,
                true,
                SourceKind::Microphone,
                ContinueWithout,
            ),
            (
                RecordingMode::ScreenAndWebcam,
                true,
                SourceKind::Screen,
                StopRecording,
            ),
            (
                RecordingMode::ScreenAndWebcam,
                true,
                SourceKind::Camera,
                StopRecording,
            ),
            (
                RecordingMode::ScreenAndWebcam,
                true,
                SourceKind::Microphone,
                ContinueWithout,
            ),
        ];
        for (mode, microphone, kind, expected) in cases {
            assert_eq!(
                config(mode, microphone).source_loss_outcome(kind),
                expected,
                "{mode:?} microphone={microphone} lost {kind:?}"
            );
        }
    }

    #[test]
    fn aggregate_records_source_loss_only_while_capturing_and_for_used_sources() {
        let mut session =
            RecordingSession::new(RecordingId::new(1), config(RecordingMode::Screen, true));
        assert!(session.record_source_loss(SourceKind::Microphone).is_err());
        session.begin_preparation().unwrap();
        session.mark_ready().unwrap();
        assert!(session.record_source_loss(SourceKind::Microphone).is_err());
        session.start().unwrap();
        assert_eq!(
            session.record_source_loss(SourceKind::Camera),
            Err(DomainError::UnexpectedSource(SourceKind::Camera))
        );
        assert_eq!(
            session.record_source_loss(SourceKind::Microphone),
            Ok(SourceLossOutcome::ContinueWithout)
        );
        session.pause().unwrap();
        assert_eq!(
            session.record_source_loss(SourceKind::Microphone),
            Ok(SourceLossOutcome::ContinueWithout)
        );
        assert_eq!(session.lost_sources(), [SourceKind::Microphone]);
        session.begin_finalization().unwrap();
        assert_eq!(
            session.record_source_loss(SourceKind::Screen),
            Ok(SourceLossOutcome::StopRecording)
        );
        assert_eq!(
            session.lost_sources(),
            [SourceKind::Microphone, SourceKind::Screen]
        );
        session.fail().unwrap();
        assert!(session.record_source_loss(SourceKind::Screen).is_err());
        session.reset().unwrap();
        assert!(session.lost_sources().is_empty());
    }

    #[test]
    fn empty_artifact_location_is_rejected() {
        assert_eq!(
            RecordingArtifact::new("  ".into()),
            Err(DomainError::EmptyArtifactLocation)
        );
    }
}
