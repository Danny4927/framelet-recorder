#![forbid(unsafe_code)]
//! TOML settings adapter implementing the core-owned `RecordingSettingsPort`
//! (roadmap C2, ADR-0023).
//!
//! The adapter owns the file format, the platform location, and the atomic
//! write; the core owns the settings types, their invariants, and defaults.
//!
//! Location (resolved from environment variables only):
//! - Linux: `$XDG_CONFIG_HOME/framelet/settings.toml`, falling back to
//!   `$HOME/.config/framelet/settings.toml` when `XDG_CONFIG_HOME` is unset,
//!   empty, or relative (XDG Base Directory specification).
//! - Windows: `%APPDATA%\Framelet\settings.toml`.
//!
//! Behaviour:
//! - `load` never fails, never panics, and never writes. A missing file gives
//!   defaults as `DefaultsNotStored`; an unreadable, oversized, malformed,
//!   unknown-version, or invalid file gives defaults with the typed error.
//! - `save` writes a temporary file in the same directory, flushes it, and
//!   renames it over `settings.toml`. A failure at any step returns
//!   `WriteFailed`, removes the temporary file, and leaves the previous file
//!   unchanged. A read-only `settings.toml` is respected and not replaced.
//! - Methods block on file I/O; call them from a worker, not the UI thread.

mod file_ops;
mod schema;

use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use framelet_core::ports::{
    LoadedSettings, RecordingSettings, RecordingSettingsPort, SettingsError,
};
use framelet_core::settings::SettingsIoFailure;

use crate::file_ops::{FileOperations, StdFileOperations};

/// File name of the settings file inside the settings directory.
pub const SETTINGS_FILE_NAME: &str = "settings.toml";

/// Largest settings file that is parsed; larger files are rejected unread.
pub const MAX_SETTINGS_FILE_BYTES: u64 = 64 * 1024;

/// Attempts to find an unused temporary name before a save gives up.
const TEMPORARY_NAME_ATTEMPTS: u32 = 16;

/// Distinguishes temporary files created by one process.
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Settings stored as one TOML file (see the crate documentation).
pub struct TomlSettingsFile {
    path: PathBuf,
    operations: Box<dyn FileOperations>,
}

impl fmt::Debug for TomlSettingsFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TomlSettingsFile")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl TomlSettingsFile {
    /// Creates an adapter for the settings file at `path`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::with_operations(path.into(), Box::new(StdFileOperations))
    }

    /// Creates an adapter for the platform settings location.
    ///
    /// Fails with [`SettingsError::LocationUnavailable`] when the required
    /// environment variables are missing, empty, or not absolute paths; the
    /// caller then uses defaults and shows the error.
    pub fn from_environment() -> Result<Self, SettingsError> {
        platform_settings_path(|name| std::env::var_os(name)).map(Self::new)
    }

    pub(crate) fn with_operations(path: PathBuf, operations: Box<dyn FileOperations>) -> Self {
        Self { path, operations }
    }

    /// Returns the settings file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the file; `Ok(None)` when it does not exist.
    ///
    /// The file type is checked *before* opening, so a FIFO, socket, or
    /// device at the settings path is rejected without an `open` that could
    /// block (opening a FIFO for reading waits for a writer). Residual race:
    /// if the path is replaced by such a node between the check and the open,
    /// the open can still block or fail; the type is checked again on the
    /// opened handle. Closing that window fully needs a non-blocking open
    /// flag, which std does not expose without platform constants (ADR-0023).
    fn read(&self) -> Result<Option<String>, SettingsError> {
        match std::fs::metadata(&self.path) {
            Ok(metadata) if metadata.is_file() => {}
            // Directory, FIFO, socket, or device. Checking here also gives
            // Windows, which reports opening a directory as access denied,
            // the same error as Linux.
            Ok(_) => return Err(SettingsError::ReadFailed(SettingsIoFailure::NotAFile)),
            // Windows reports a file in place of the settings directory as
            // "not found"; report it as on Linux instead of as no settings.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if self.path.parent().is_some_and(Path::is_file) {
                    return Err(SettingsError::ReadFailed(SettingsIoFailure::NotAFile));
                }
                return Ok(None);
            }
            Err(error) => return Err(SettingsError::ReadFailed(io_failure(&error))),
        }
        let file = match File::open(&self.path) {
            Ok(file) => file,
            // Removed between the check and the open: nothing stored.
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(SettingsError::ReadFailed(io_failure(&error))),
        };
        let metadata = file
            .metadata()
            .map_err(|error| SettingsError::ReadFailed(io_failure(&error)))?;
        if !metadata.is_file() {
            return Err(SettingsError::ReadFailed(SettingsIoFailure::NotAFile));
        }
        let mut bytes = Vec::new();
        file.take(MAX_SETTINGS_FILE_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| SettingsError::ReadFailed(io_failure(&error)))?;
        let within_limit =
            u64::try_from(bytes.len()).is_ok_and(|len| len <= MAX_SETTINGS_FILE_BYTES);
        if !within_limit {
            return Err(SettingsError::TooLarge {
                limit_bytes: MAX_SETTINGS_FILE_BYTES,
            });
        }
        String::from_utf8(bytes).map(Some).map_err(|error| {
            let offset = error.utf8_error().valid_up_to();
            SettingsError::Malformed {
                line: schema::line_at(error.as_bytes(), offset),
            }
        })
    }

    /// Refuses to replace a read-only settings file or a directory.
    ///
    /// Any other inspection error (for example the parent is a file) is left
    /// to the write steps, which report it. A symlink, FIFO, or socket at the
    /// path is not refused: the final rename replaces the node itself and
    /// never writes through it. This is a check-then-act: a change to the
    /// path between this check and the rename (for example another process
    /// making the file read-only) is not detected; the rename then replaces
    /// whatever is there with the complete new file (ADR-0023).
    fn check_replaceable(&self) -> Result<(), SettingsError> {
        match std::fs::symlink_metadata(&self.path) {
            // Checked explicitly: Windows would report replacing a directory
            // as access denied rather than as the wrong file type.
            Ok(metadata) if metadata.is_dir() => {
                Err(SettingsError::WriteFailed(SettingsIoFailure::NotAFile))
            }
            Ok(metadata) if metadata.is_file() && metadata.permissions().readonly() => {
                Err(SettingsError::WriteFailed(SettingsIoFailure::ReadOnly))
            }
            Ok(_) | Err(_) => Ok(()),
        }
    }

    /// Creates an exclusive temporary file next to the settings file.
    fn create_temporary(&self, directory: &Path) -> Result<(PathBuf, File), SettingsError> {
        for _ in 0..TEMPORARY_NAME_ATTEMPTS {
            let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
            let name = format!(
                ".{SETTINGS_FILE_NAME}.{}.{sequence}.tmp",
                std::process::id()
            );
            let path = directory.join(name);
            match self.operations.create_new(&path) {
                Ok(file) => return Ok((path, file)),
                // A stale temporary file of an earlier process: try the next name.
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(SettingsError::WriteFailed(io_failure(&error))),
            }
        }
        Err(SettingsError::WriteFailed(SettingsIoFailure::Other))
    }

    /// Writes, flushes, closes, and renames the temporary file into place.
    fn replace_with(&self, temporary: &Path, mut file: File, text: &str) -> io::Result<()> {
        self.operations.write_all(&mut file, text.as_bytes())?;
        self.operations.sync_all(&file)?;
        drop(file);
        self.operations.rename(temporary, &self.path)
    }
}

impl RecordingSettingsPort for TomlSettingsFile {
    fn load(&self) -> LoadedSettings {
        match self.read() {
            Ok(None) => LoadedSettings::defaults_not_stored(),
            Ok(Some(text)) => match schema::decode(&text) {
                Ok(settings) => LoadedSettings::stored(settings),
                Err(error) => LoadedSettings::defaults_after_error(error),
            },
            Err(error) => LoadedSettings::defaults_after_error(error),
        }
    }

    fn save(&self, settings: &RecordingSettings) -> Result<(), SettingsError> {
        let text = schema::encode(settings)?;
        let directory = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(SettingsError::WriteFailed(SettingsIoFailure::NotAFile))?;
        self.check_replaceable()?;
        self.operations
            .create_dir_all(directory)
            .map_err(|error| SettingsError::WriteFailed(io_failure(&error)))?;
        let (temporary, file) = self.create_temporary(directory)?;
        self.replace_with(&temporary, file, &text).map_err(|error| {
            // The primary error is what the user must see. If removing the
            // temporary file also fails, it stays behind under its
            // `.settings.toml.*.tmp` name, which `load` never reads.
            let _ = self.operations.remove_file(&temporary);
            SettingsError::WriteFailed(io_failure(&error))
        })
    }
}

/// Translates an I/O error into the core's adapter-neutral failure kind.
fn io_failure(error: &io::Error) -> SettingsIoFailure {
    match error.kind() {
        io::ErrorKind::PermissionDenied => SettingsIoFailure::PermissionDenied,
        io::ErrorKind::ReadOnlyFilesystem => SettingsIoFailure::ReadOnly,
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => SettingsIoFailure::StorageFull,
        io::ErrorKind::NotADirectory
        | io::ErrorKind::IsADirectory
        | io::ErrorKind::AlreadyExists => SettingsIoFailure::NotAFile,
        _ => SettingsIoFailure::Other,
    }
}

/// An environment value usable as a base directory: set, non-empty, absolute.
fn absolute_directory(value: Option<OsString>) -> Option<PathBuf> {
    value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Linux location: `$XDG_CONFIG_HOME/framelet`, else `$HOME/.config/framelet`.
fn linux_settings_path(
    lookup: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, SettingsError> {
    absolute_directory(lookup("XDG_CONFIG_HOME"))
        .or_else(|| absolute_directory(lookup("HOME")).map(|home| home.join(".config")))
        .map(|base| base.join("framelet").join(SETTINGS_FILE_NAME))
        .ok_or(SettingsError::LocationUnavailable)
}

/// Windows location: `%APPDATA%\Framelet`.
fn windows_settings_path(
    lookup: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, SettingsError> {
    absolute_directory(lookup("APPDATA"))
        .map(|base| base.join("Framelet").join(SETTINGS_FILE_NAME))
        .ok_or(SettingsError::LocationUnavailable)
}

/// The settings path for the running platform.
fn platform_settings_path(
    lookup: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, SettingsError> {
    if cfg!(windows) {
        windows_settings_path(lookup)
    } else {
        linux_settings_path(lookup)
    }
}

#[cfg(test)]
mod tests;
