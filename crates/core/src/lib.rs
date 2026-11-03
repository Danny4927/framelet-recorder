#![forbid(unsafe_code)]
//! Recording modes and lifecycle state.

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

/// Lifecycle owned by the application core.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingState {
    /// No active recording.
    Idle,
    /// Sources and output are prepared.
    Preparing,
    /// Sources are ready.
    Ready,
    /// Recording is active.
    Recording,
    /// Recording is paused.
    Paused,
    /// Output is being finalized.
    Finalizing,
    /// Output finalized successfully.
    Completed,
    /// An operation failed.
    Failed,
}

/// Returns whether a lifecycle transition is permitted.
pub fn can_transition(from: RecordingState, to: RecordingState) -> bool {
    use RecordingState::{Completed, Failed, Finalizing, Idle, Paused, Preparing, Ready, Recording};
    matches!((from, to),
        (Idle, Preparing) | (Preparing, Ready) | (Preparing, Idle) | (Preparing, Failed)
        | (Ready, Recording) | (Ready, Idle) | (Recording, Paused)
        | (Recording, Finalizing) | (Recording, Failed) | (Paused, Recording)
        | (Paused, Finalizing) | (Paused, Failed) | (Finalizing, Completed)
        | (Finalizing, Failed) | (Completed, Idle) | (Failed, Idle))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_can_pause_and_resume() {
        assert!(can_transition(RecordingState::Recording, RecordingState::Paused));
        assert!(can_transition(RecordingState::Paused, RecordingState::Recording));
    }
    #[test]
    fn idle_cannot_jump_to_completed() {
        assert!(!can_transition(RecordingState::Idle, RecordingState::Completed));
    }
}
