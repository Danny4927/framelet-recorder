#![forbid(unsafe_code)]
//! Runs the shared `RecordingSettingsPort` contract suites (ADR-0023) against
//! `TomlSettingsFile` in a fresh temporary directory per subject.
//!
//! The same suites run against the core's in-memory fake
//! (`framelet_core::contract::tests`), so fake and adapter cannot drift.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use framelet_core::contract::settings::{self, SettingsSubject};
use framelet_core::ports::RecordingSettingsPort;
use framelet_settings::TomlSettingsFile;

/// Distinguishes subjects created by one test process.
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// TOML adapter plus a directory it owns; the directory is removed on drop.
struct TomlSubject {
    directory: PathBuf,
    settings: TomlSettingsFile,
}

impl TomlSubject {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "framelet-settings-contract-{}-{}-{nonce}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        let settings = TomlSettingsFile::new(directory.join("framelet").join("settings.toml"));
        Self {
            directory,
            settings,
        }
    }

    fn set_settings_read_only(&self, read_only: bool) {
        let path = self.settings.path();
        let Ok(metadata) = fs::metadata(path) else {
            return;
        };
        let mut permissions = metadata.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(if read_only { 0o400 } else { 0o600 });
        }
        #[cfg(windows)]
        permissions.set_readonly(read_only);
        fs::set_permissions(path, permissions).expect("change settings file permissions");
    }
}

impl Drop for TomlSubject {
    fn drop(&mut self) {
        self.set_settings_read_only(false);
        let _ = fs::remove_dir_all(&self.directory);
    }
}

impl SettingsSubject for TomlSubject {
    fn settings(&self) -> &dyn RecordingSettingsPort {
        &self.settings
    }

    fn stored_bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.settings.path()).ok()
    }

    fn store_unusable_settings(&mut self) {
        let path = self.settings.path();
        fs::create_dir_all(path.parent().expect("settings directory")).expect("create directory");
        fs::write(
            path,
            b"schema_version = 1\noutput_directory = [unterminated\n",
        )
        .expect("write unusable settings");
    }

    /// A read-only settings file, which the adapter refuses to replace on
    /// every platform and for every user, so this never needs to skip.
    fn make_saves_fail(&mut self) -> bool {
        if fs::metadata(self.settings.path()).is_err() {
            return false;
        }
        self.set_settings_read_only(true);
        true
    }
}

#[test]
fn toml_settings_missing_settings_load_as_defaults() {
    settings::missing_settings_load_as_defaults(TomlSubject::new);
}

#[test]
fn toml_settings_saved_settings_round_trip() {
    settings::saved_settings_round_trip(TomlSubject::new);
}

#[test]
fn toml_settings_unusable_settings_load_as_defaults_with_error() {
    settings::unusable_settings_load_as_defaults_with_error(TomlSubject::new);
}

#[test]
fn toml_settings_failed_save_keeps_previous_settings() {
    settings::failed_save_keeps_previous_settings(TomlSubject::new);
}

#[test]
fn toml_settings_pass_the_whole_settings_suite() {
    settings::run_all(TomlSubject::new);
}
