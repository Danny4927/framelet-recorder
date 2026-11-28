//! Contract suite for [`RecordingSettingsPort`] (ADR-0023).
//!
//! Every suite takes a factory that returns a fresh [`SettingsSubject`] with
//! nothing stored, and panics with a descriptive message when the adapter
//! breaks the contract documented on the port.

use crate::ports::{
    LoadedSettings, RecordingSettings, RecordingSettingsPort, SettingsError, SettingsOrigin,
};
use crate::settings::OutputDirectory;

/// A settings adapter under test plus the means to tamper with its store.
pub trait SettingsSubject {
    /// Returns the adapter under test.
    fn settings(&self) -> &dyn RecordingSettingsPort;
    /// Returns the raw stored representation, or `None` when nothing is stored.
    fn stored_bytes(&self) -> Option<Vec<u8>>;
    /// Replaces the stored representation with content the adapter must reject.
    fn store_unusable_settings(&mut self);
    /// Makes every later `save` fail. Returns `false` when the environment
    /// cannot make it fail (for example a process whose privileges bypass
    /// file permissions); the suite then skips that check and says so.
    fn make_saves_fail(&mut self) -> bool;
}

#[cfg(windows)]
const EXAMPLE_DIRECTORY: &str = r"C:\Users\contract\Videos\Recordings";
#[cfg(not(windows))]
const EXAMPLE_DIRECTORY: &str = "/home/contract/Videos/Recordings";

/// Non-default settings, so a round trip cannot pass by returning defaults.
#[must_use]
pub fn example_settings() -> RecordingSettings {
    match OutputDirectory::new(EXAMPLE_DIRECTORY) {
        Ok(directory) => RecordingSettings {
            output_directory: Some(directory),
            microphone_enabled_by_default: true,
        },
        Err(violation) => panic!("example directory rejected: {violation}"),
    }
}

fn save_or_panic(subject: &impl SettingsSubject, settings: &RecordingSettings) {
    if let Err(error) = subject.settings().save(settings) {
        panic!("save failed: {error}");
    }
}

/// With nothing stored, `load` returns defaults as `DefaultsNotStored`, which
/// is not an error, and does not create stored settings.
pub fn missing_settings_load_as_defaults<S: SettingsSubject>(mut make: impl FnMut() -> S) {
    let subject = make();
    assert_eq!(
        subject.settings().load(),
        LoadedSettings::defaults_not_stored()
    );
    assert_eq!(subject.settings().load().error(), None);
    assert_eq!(
        subject.stored_bytes(),
        None,
        "load must not create stored settings"
    );
}

/// A saved value is loaded back exactly as `Stored`, and a later save
/// replaces it completely.
pub fn saved_settings_round_trip<S: SettingsSubject>(mut make: impl FnMut() -> S) {
    let subject = make();
    let example = example_settings();
    save_or_panic(&subject, &example);
    assert_eq!(subject.settings().load(), LoadedSettings::stored(example));

    let defaults = RecordingSettings::default();
    save_or_panic(&subject, &defaults);
    assert_eq!(
        subject.settings().load(),
        LoadedSettings::stored(defaults),
        "a second save must replace the first"
    );
}

/// Unusable stored settings load as defaults together with a typed error,
/// and loading leaves them untouched for the user to inspect.
pub fn unusable_settings_load_as_defaults_with_error<S: SettingsSubject>(
    mut make: impl FnMut() -> S,
) {
    let mut subject = make();
    subject.store_unusable_settings();
    let before = subject.stored_bytes();
    assert!(before.is_some(), "the subject must store unusable settings");

    let loaded = subject.settings().load();
    assert_eq!(loaded.settings, RecordingSettings::default());
    assert!(
        matches!(loaded.origin, SettingsOrigin::DefaultsAfterError(_)),
        "unusable settings must report an error, got {:?}",
        loaded.origin
    );
    assert_eq!(
        subject.stored_bytes(),
        before,
        "load must never modify or delete stored settings"
    );
}

/// A failed save returns `WriteFailed` and leaves the previously stored
/// settings unchanged and loadable.
pub fn failed_save_keeps_previous_settings<S: SettingsSubject>(mut make: impl FnMut() -> S) {
    let mut subject = make();
    let example = example_settings();
    save_or_panic(&subject, &example);
    let before = subject.stored_bytes();

    if !subject.make_saves_fail() {
        eprintln!(
            "skipped failed_save_keeps_previous_settings: this process can write despite \
             the subject's restriction (for example it runs as root)"
        );
        return;
    }
    let result = subject.settings().save(&RecordingSettings::default());
    assert!(
        matches!(result, Err(SettingsError::WriteFailed(_))),
        "failed save must return WriteFailed, got {result:?}"
    );
    assert_eq!(
        subject.stored_bytes(),
        before,
        "a failed save must not touch stored settings"
    );
    assert_eq!(subject.settings().load(), LoadedSettings::stored(example));
}

/// Runs every settings suite.
pub fn run_all<S: SettingsSubject>(mut make: impl FnMut() -> S) {
    missing_settings_load_as_defaults(&mut make);
    saved_settings_round_trip(&mut make);
    unusable_settings_load_as_defaults_with_error(&mut make);
    failed_save_keeps_previous_settings(&mut make);
}
