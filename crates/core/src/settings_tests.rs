use super::*;

#[cfg(unix)]
const ABSOLUTE: &str = "/home/user/Videos/Framelet";
#[cfg(windows)]
const ABSOLUTE: &str = r"C:\Users\user\Videos\Framelet";

#[cfg(unix)]
fn absolute_with_len(len: usize) -> String {
    let mut value = String::from("/");
    value.push_str(&"a".repeat(len - 1));
    value
}

#[cfg(windows)]
fn absolute_with_len(len: usize) -> String {
    let mut value = String::from(r"C:\");
    value.push_str(&"a".repeat(len - 3));
    value
}

#[test]
fn output_directory_accepts_an_absolute_path_and_preserves_it() {
    let directory = OutputDirectory::new(ABSOLUTE).unwrap();
    assert_eq!(directory.as_str(), ABSOLUTE);
    assert_eq!(directory.as_path(), Path::new(ABSOLUTE));
}

#[test]
fn output_directory_length_limit_is_inclusive() {
    let at_limit = absolute_with_len(MAX_OUTPUT_DIRECTORY_BYTES);
    assert_eq!(at_limit.len(), MAX_OUTPUT_DIRECTORY_BYTES);
    assert!(OutputDirectory::new(at_limit).is_ok());
    let over_limit = absolute_with_len(MAX_OUTPUT_DIRECTORY_BYTES + 1);
    assert_eq!(
        OutputDirectory::new(over_limit),
        Err(OutputDirectoryViolation::TooLong)
    );
}

#[test]
fn output_directory_rejects_every_invariant_violation() {
    let cases: [(String, OutputDirectoryViolation); 7] = [
        (String::new(), OutputDirectoryViolation::Empty),
        ("  \t".into(), OutputDirectoryViolation::Empty),
        (
            format!("{ABSOLUTE}\0x"),
            OutputDirectoryViolation::ControlCharacter,
        ),
        (
            format!("{ABSOLUTE}\nx"),
            OutputDirectoryViolation::ControlCharacter,
        ),
        (
            "Videos/Framelet".into(),
            OutputDirectoryViolation::NotAbsolute,
        ),
        ("Framelet".into(), OutputDirectoryViolation::NotAbsolute),
        (
            format!(
                "{ABSOLUTE}{}..{}x",
                std::path::MAIN_SEPARATOR,
                std::path::MAIN_SEPARATOR
            ),
            OutputDirectoryViolation::ParentComponent,
        ),
    ];
    for (value, expected) in cases {
        assert_eq!(
            OutputDirectory::new(value.clone()),
            Err(expected),
            "{value:?}"
        );
    }
}

#[test]
fn defaults_use_the_platform_directory_and_keep_the_microphone_off() {
    let defaults = RecordingSettings::default();
    assert_eq!(defaults.output_directory, None);
    assert!(!defaults.microphone_enabled_by_default);
}

#[test]
fn loaded_settings_report_an_error_only_after_a_failed_load() {
    let stored = RecordingSettings {
        output_directory: Some(OutputDirectory::new(ABSOLUTE).unwrap()),
        microphone_enabled_by_default: true,
    };
    let loaded = LoadedSettings::stored(stored.clone());
    assert_eq!(loaded.settings, stored);
    assert_eq!(loaded.origin, SettingsOrigin::Stored);
    assert_eq!(loaded.error(), None);

    let missing = LoadedSettings::defaults_not_stored();
    assert_eq!(missing.settings, RecordingSettings::default());
    assert_eq!(missing.origin, SettingsOrigin::DefaultsNotStored);
    assert_eq!(missing.error(), None);

    let error = SettingsError::Malformed { line: None };
    let rejected = LoadedSettings::defaults_after_error(error);
    assert_eq!(rejected.settings, RecordingSettings::default());
    assert_eq!(rejected.origin, SettingsOrigin::DefaultsAfterError(error));
    assert_eq!(rejected.error(), Some(&error));
}

#[test]
fn settings_errors_are_actionable_messages() {
    let line = NonZeroUsize::new(3);
    assert_eq!(
        SettingsError::Malformed { line }.to_string(),
        "stored settings are malformed (line 3)"
    );
    assert_eq!(
        SettingsError::UnsupportedSchemaVersion {
            found: 2,
            supported: 1
        }
        .to_string(),
        "stored settings use schema version 2; this version of Framelet supports 1"
    );
    assert_eq!(
        SettingsError::InvalidOutputDirectory(OutputDirectoryViolation::NotAbsolute).to_string(),
        "stored settings are invalid: output directory must be an absolute path"
    );
    assert_eq!(
        SettingsError::WriteFailed(SettingsIoFailure::PermissionDenied).to_string(),
        "settings could not be saved (permission denied); the previous settings are unchanged"
    );
    assert_eq!(
        SettingsError::TooLarge { limit_bytes: 65536 }.to_string(),
        "stored settings are larger than 65536 bytes"
    );
}
