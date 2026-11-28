//! Recording settings: the persisted, user-editable preferences of the
//! Recording context, their invariants and defaults, and the typed outcomes
//! of loading and storing them (ADR-0023).
//!
//! The core owns the rules; the settings adapter owns serialization and the
//! platform location and maps its file format onto these types at its
//! boundary. None of these types know about a file format.

use std::fmt;
use std::num::NonZeroUsize;
use std::path::{Component, Path};

/// Longest accepted output directory, in UTF-8 bytes.
///
/// Matches Linux `PATH_MAX` and is far above the classic Windows `MAX_PATH`;
/// it bounds what a hand-edited settings file can make the application hold.
pub const MAX_OUTPUT_DIRECTORY_BYTES: usize = 4096;

/// Why a proposed output directory violates the output-directory invariant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputDirectoryViolation {
    /// The directory is empty or whitespace only.
    Empty,
    /// The directory is longer than [`MAX_OUTPUT_DIRECTORY_BYTES`].
    TooLong,
    /// The directory contains a control character (including NUL or a line break).
    ControlCharacter,
    /// The directory is not an absolute path on this platform.
    NotAbsolute,
    /// The directory contains a `..` component.
    ParentComponent,
}

impl fmt::Display for OutputDirectoryViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "output directory must not be empty"),
            Self::TooLong => write!(
                f,
                "output directory must not exceed {MAX_OUTPUT_DIRECTORY_BYTES} bytes"
            ),
            Self::ControlCharacter => {
                write!(f, "output directory must not contain control characters")
            }
            Self::NotAbsolute => write!(f, "output directory must be an absolute path"),
            Self::ParentComponent => {
                write!(f, "output directory must not contain '..' components")
            }
        }
    }
}

/// A user-chosen recording output directory (value object).
///
/// Invariant: non-empty, at most [`MAX_OUTPUT_DIRECTORY_BYTES`] bytes, free of
/// control characters, absolute on the running platform, and without `..`
/// components. The check is lexical; whether the directory exists or is
/// writable is decided by the storage adapter when it is used.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OutputDirectory(String);

impl OutputDirectory {
    /// Validates a user-facing directory path.
    pub fn new(value: impl Into<String>) -> Result<Self, OutputDirectoryViolation> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(OutputDirectoryViolation::Empty);
        }
        if value.len() > MAX_OUTPUT_DIRECTORY_BYTES {
            return Err(OutputDirectoryViolation::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(OutputDirectoryViolation::ControlCharacter);
        }
        let path = Path::new(&value);
        if !path.is_absolute() {
            return Err(OutputDirectoryViolation::NotAbsolute);
        }
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(OutputDirectoryViolation::ParentComponent);
        }
        Ok(Self(value))
    }

    /// Returns the directory as the user-facing string it was created from.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the directory as a path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

/// Persisted, user-editable settings relevant to recording (value object).
///
/// [`Default`] is the safe default used when nothing is stored or the stored
/// settings are unusable: no explicit output directory (the platform default
/// Videos/Framelet directory applies, REQ-STOR-06) and the optional
/// microphone off (privacy by default, Spec §1.3).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecordingSettings {
    /// Explicitly chosen output directory; `None` means the platform default.
    pub output_directory: Option<OutputDirectory>,
    /// Whether the optional microphone is enabled by default in modes that
    /// permit it. Microphone-only mode always records the microphone.
    pub microphone_enabled_by_default: bool,
}

/// How an I/O operation on the settings store failed, without native types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsIoFailure {
    /// The operating system denied access.
    PermissionDenied,
    /// The settings file or file system is read-only.
    ReadOnly,
    /// The disk or quota is full.
    StorageFull,
    /// A path component is not what the store expects (for example the
    /// settings path is a directory, or its parent is a file).
    NotAFile,
    /// Any other I/O failure.
    Other,
}

impl fmt::Display for SettingsIoFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::ReadOnly => write!(f, "read-only"),
            Self::StorageFull => write!(f, "no space left"),
            Self::NotAFile => write!(f, "unexpected file type at the settings location"),
            Self::Other => write!(f, "input/output error"),
        }
    }
}

/// Typed settings failure; every variant is meant to be shown to the user.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsError {
    /// The platform settings location could not be determined.
    LocationUnavailable,
    /// The stored settings exceed the store's size limit and were not parsed.
    TooLarge {
        /// Size limit in bytes.
        limit_bytes: u64,
    },
    /// The stored settings are not well-formed (for example invalid TOML or
    /// not UTF-8).
    Malformed {
        /// 1-based line of the first problem, when known.
        line: Option<NonZeroUsize>,
    },
    /// The stored settings carry no schema version.
    MissingSchemaVersion,
    /// The stored settings were written for a schema this build does not support.
    UnsupportedSchemaVersion {
        /// The version found in the stored settings.
        found: i64,
        /// The only version this build reads and writes.
        supported: u32,
    },
    /// The stored settings do not match the schema (unknown key, wrong type).
    SchemaMismatch {
        /// 1-based line of the first problem, when known.
        line: Option<NonZeroUsize>,
    },
    /// A stored output directory violates its invariant.
    InvalidOutputDirectory(OutputDirectoryViolation),
    /// Reading the stored settings failed.
    ReadFailed(SettingsIoFailure),
    /// Writing failed; the previously stored settings are unchanged.
    WriteFailed(SettingsIoFailure),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocationUnavailable => {
                write!(f, "the settings location could not be determined")
            }
            Self::TooLarge { limit_bytes } => {
                write!(f, "stored settings are larger than {limit_bytes} bytes")
            }
            Self::Malformed { line: Some(line) } => {
                write!(f, "stored settings are malformed (line {line})")
            }
            Self::Malformed { line: None } => write!(f, "stored settings are malformed"),
            Self::MissingSchemaVersion => {
                write!(f, "stored settings have no schema_version")
            }
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                f,
                "stored settings use schema version {found}; this version of Framelet supports {supported}"
            ),
            Self::SchemaMismatch { line: Some(line) } => write!(
                f,
                "stored settings contain an unknown key or a value of the wrong type (line {line})"
            ),
            Self::SchemaMismatch { line: None } => write!(
                f,
                "stored settings contain an unknown key or a value of the wrong type"
            ),
            Self::InvalidOutputDirectory(violation) => {
                write!(f, "stored settings are invalid: {violation}")
            }
            Self::ReadFailed(failure) => write!(f, "settings could not be read: {failure}"),
            Self::WriteFailed(failure) => write!(
                f,
                "settings could not be saved ({failure}); the previous settings are unchanged"
            ),
        }
    }
}

impl std::error::Error for SettingsError {}

/// Where loaded settings came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsOrigin {
    /// The stored settings were read and are valid.
    Stored,
    /// Nothing is stored yet; defaults apply. This is not a failure.
    DefaultsNotStored,
    /// The stored settings are unusable; defaults apply and the error must be
    /// shown. The stored settings were left unchanged.
    DefaultsAfterError(SettingsError),
}

/// The outcome of loading settings: always usable settings plus their origin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSettings {
    /// Settings to use.
    pub settings: RecordingSettings,
    /// Where `settings` came from.
    pub origin: SettingsOrigin,
}

impl LoadedSettings {
    /// Valid stored settings.
    #[must_use]
    pub const fn stored(settings: RecordingSettings) -> Self {
        Self {
            settings,
            origin: SettingsOrigin::Stored,
        }
    }

    /// Defaults because nothing is stored yet.
    #[must_use]
    pub fn defaults_not_stored() -> Self {
        Self {
            settings: RecordingSettings::default(),
            origin: SettingsOrigin::DefaultsNotStored,
        }
    }

    /// Defaults because the stored settings could not be used.
    #[must_use]
    pub fn defaults_after_error(error: SettingsError) -> Self {
        Self {
            settings: RecordingSettings::default(),
            origin: SettingsOrigin::DefaultsAfterError(error),
        }
    }

    /// Returns the error the user must be shown, if any.
    #[must_use]
    pub const fn error(&self) -> Option<&SettingsError> {
        match &self.origin {
            SettingsOrigin::DefaultsAfterError(error) => Some(error),
            SettingsOrigin::Stored | SettingsOrigin::DefaultsNotStored => None,
        }
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
