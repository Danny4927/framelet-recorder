#![forbid(unsafe_code)]
//! Safe, backend-independent capture contracts.

use std::time::Duration;

/// Stable identifier assigned by a capture adapter.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SourceId(String);

impl SourceId {
    /// Creates an identifier after checking it is non-empty.
    pub fn new(value: impl Into<String>) -> Result<Self, CaptureError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CaptureError::InvalidSourceId);
        }
        Ok(Self(value))
    }

    /// Returns the identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// User-selectable capture source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceDescriptor {
    /// Stable adapter-local identifier.
    pub id: SourceId,
    /// Human-readable display name.
    pub name: String,
}

/// Metadata for a captured video frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoFrameMetadata {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bytes per row.
    pub stride: u32,
    /// Monotonic timestamp relative to recording start.
    pub timestamp: Duration,
    /// Monotonically increasing sequence number.
    pub sequence: u64,
}

/// Typed capture errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureError {
    /// Source identifier was empty.
    InvalidSourceId,
    /// Source is no longer available.
    SourceUnavailable,
    /// OS denied capture permission.
    PermissionDenied,
    /// User cancelled source picker.
    SelectionCancelled,
    /// Requested format is unsupported.
    UnsupportedFormat,
    /// Backend-specific failure.
    BackendFailure(String),
}

/// Safe lifecycle contract for a video source.
pub trait VideoSource {
    /// Lists available sources.
    fn sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError>;
    /// Starts the selected source.
    fn start(&mut self, source: &SourceId) -> Result<(), CaptureError>;
    /// Stops capture and releases the source.
    fn stop(&mut self) -> Result<(), CaptureError>;
}

/// Safe lifecycle contract for a microphone source.
pub trait AudioSource {
    /// Lists available microphone sources.
    fn sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError>;
    /// Starts the selected source.
    fn start(&mut self, source: &SourceId) -> Result<(), CaptureError>;
    /// Stops capture and releases the source.
    fn stop(&mut self) -> Result<(), CaptureError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_rejects_empty_values() {
        assert_eq!(SourceId::new(" "), Err(CaptureError::InvalidSourceId));
    }

    #[test]
    fn source_id_preserves_valid_value() {
        let id = SourceId::new("display-1").expect("valid test identifier");
        assert_eq!(id.as_str(), "display-1");
    }
}
