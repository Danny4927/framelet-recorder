//! Adapter tests: schema mapping, real-file behaviour, injected failures at
//! every step of the atomic write, and platform location resolution.

use std::fs;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use framelet_core::ports::SettingsOrigin;
use framelet_core::settings::{
    MAX_OUTPUT_DIRECTORY_BYTES, OutputDirectory, OutputDirectoryViolation,
};

use super::*;

// ------------------------------------------------------------------ helpers

#[cfg(windows)]
const DIRECTORY: &str = r"C:\Users\user\Videos\Framelet";
#[cfg(not(windows))]
const DIRECTORY: &str = "/home/user/Videos/Framelet";

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// A fresh directory removed (after restoring write access) on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "framelet-settings-test-{}-{}-{nonce}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    /// `<temp>/framelet/settings.toml`; the `framelet` directory is not created.
    fn settings_path(&self) -> PathBuf {
        self.0.join("framelet").join(SETTINGS_FILE_NAME)
    }

    fn store(&self) -> TomlSettingsFile {
        TomlSettingsFile::new(self.settings_path())
    }

    /// Writes `bytes` as the settings file, creating its directory.
    fn write_settings(&self, bytes: &[u8]) -> PathBuf {
        let path = self.settings_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    /// Names of every entry in the settings directory, sorted.
    fn entries(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.settings_path().parent().unwrap()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        make_writable(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
fn make_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                make_writable(&entry.path());
            }
        }
    } else if metadata.is_file() {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

#[cfg(windows)]
// Clears the Windows read-only attribute of a test file; the lint's concern
// (world-writable files on Unix) does not apply to this Windows-only helper.
#[allow(clippy::permissions_set_readonly_false)]
fn make_writable(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                make_writable(&entry.path());
            }
        }
    } else if metadata.permissions().readonly() {
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        let _ = fs::set_permissions(path, permissions);
    }
}

fn read_only(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).unwrap();
}

fn example() -> RecordingSettings {
    RecordingSettings {
        output_directory: Some(OutputDirectory::new(DIRECTORY).unwrap()),
        microphone_enabled_by_default: true,
    }
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

fn line(number: usize) -> Option<std::num::NonZeroUsize> {
    std::num::NonZeroUsize::new(number)
}

fn rejected(error: SettingsError) -> LoadedSettings {
    LoadedSettings::defaults_after_error(error)
}

// ------------------------------------------------------------------- schema

#[test]
fn decodes_a_complete_file() {
    let text = format!(
        "schema_version = 1\nmicrophone_enabled_by_default = true\noutput_directory = {}\n",
        toml_string(DIRECTORY)
    );
    assert_eq!(schema::decode(&text), Ok(example()));
}

#[test]
fn absent_optional_keys_take_the_core_defaults() {
    assert_eq!(
        schema::decode("schema_version = 1\n"),
        Ok(RecordingSettings::default())
    );
}

#[test]
fn malformed_toml_reports_its_line() {
    for (text, expected) in [
        ("schema_version = 1\nmicrophone_enabled_by_default = \n", 2),
        ("schema_version = 1\n\n[unterminated\n", 3),
        ("schema_version = 1\nschema_version = 1\n", 2),
        ("= 1\n", 1),
    ] {
        assert_eq!(
            schema::decode(text),
            Err(SettingsError::Malformed {
                line: line(expected)
            }),
            "{text:?}"
        );
    }
}

#[test]
fn missing_schema_version_is_a_typed_error() {
    for text in [
        "",
        "# only a comment\n",
        "microphone_enabled_by_default = true\n",
    ] {
        assert_eq!(
            schema::decode(text),
            Err(SettingsError::MissingSchemaVersion),
            "{text:?}"
        );
    }
}

#[test]
fn unknown_and_future_schema_versions_are_rejected() {
    for found in [0, 2, 99, -1, i64::MAX, i64::MIN] {
        let text = format!("schema_version = {found}\nfuture_key = \"x\"\n");
        assert_eq!(
            schema::decode(&text),
            Err(SettingsError::UnsupportedSchemaVersion {
                found,
                supported: 1
            }),
            "{found}"
        );
    }
}

#[test]
fn unknown_keys_and_wrong_types_are_schema_mismatches_with_their_line() {
    for (text, expected) in [
        (
            "schema_version = 1\nmicrophone_enabled_by_defualt = true\n",
            2,
        ),
        (
            "schema_version = 1\nmicrophone_enabled_by_default = \"yes\"\n",
            2,
        ),
        ("schema_version = 1\n\noutput_directory = 7\n", 3),
        ("schema_version = \"1\"\n", 1),
        ("schema_version = 1.0\n", 1),
        ("schema_version = 1\n[output_directory]\npath = \"/x\"\n", 2),
    ] {
        assert_eq!(
            schema::decode(text),
            Err(SettingsError::SchemaMismatch {
                line: line(expected)
            }),
            "{text:?}"
        );
    }
}

#[test]
fn out_of_range_output_directories_are_rejected_not_clamped() {
    let too_long = format!(
        "{DIRECTORY}{}{}",
        std::path::MAIN_SEPARATOR,
        "a".repeat(MAX_OUTPUT_DIRECTORY_BYTES)
    );
    let parent = format!(
        "{DIRECTORY}{}..{}x",
        std::path::MAIN_SEPARATOR,
        std::path::MAIN_SEPARATOR
    );
    for (value, violation) in [
        ("", OutputDirectoryViolation::Empty),
        ("   ", OutputDirectoryViolation::Empty),
        ("Videos/Framelet", OutputDirectoryViolation::NotAbsolute),
        (too_long.as_str(), OutputDirectoryViolation::TooLong),
        (
            "/home/user/\u{0}x",
            OutputDirectoryViolation::ControlCharacter,
        ),
        (
            "/home/user/\u{1b}[31m",
            OutputDirectoryViolation::ControlCharacter,
        ),
        (parent.as_str(), OutputDirectoryViolation::ParentComponent),
    ] {
        let text = format!(
            "schema_version = 1\noutput_directory = {}\n",
            toml_string(value)
        );
        assert_eq!(
            schema::decode(&text),
            Err(SettingsError::InvalidOutputDirectory(violation)),
            "{value:?}"
        );
    }
}

#[test]
fn deeply_nested_input_is_rejected_without_crashing() {
    // Stays below the file size limit, so it reaches the parser.
    let depth = 30_000;
    let nested = format!(
        "schema_version = 1\nx = {}{}\n",
        "[".repeat(depth),
        "]".repeat(depth)
    );
    assert!(u64::try_from(nested.len()).unwrap() < MAX_SETTINGS_FILE_BYTES);
    assert!(matches!(
        schema::decode(&nested),
        Err(SettingsError::Malformed { .. } | SettingsError::SchemaMismatch { .. })
    ));
    let dotted = format!("schema_version = 1\n{} = 1\n", vec!["a"; 20_000].join("."));
    assert!(matches!(
        schema::decode(&dotted),
        Err(SettingsError::Malformed { .. } | SettingsError::SchemaMismatch { .. })
    ));
}

#[test]
fn encoded_settings_round_trip_including_special_characters() {
    let mut special = String::from(DIRECTORY);
    special.push(std::path::MAIN_SEPARATOR);
    special.push_str("Vidéos \"quoted\" 'single' \\ # = [x] ü 録画");
    let settings = RecordingSettings {
        output_directory: Some(OutputDirectory::new(special).unwrap()),
        microphone_enabled_by_default: false,
    };
    for value in [settings, example(), RecordingSettings::default()] {
        let text = schema::encode(&value).unwrap();
        assert_eq!(schema::decode(&text), Ok(value));
    }
}

#[test]
fn encoded_file_carries_the_schema_version_and_omits_the_default_directory() {
    let text = schema::encode(&RecordingSettings::default()).unwrap();
    assert!(text.starts_with("# Framelet settings (schema version 1)"));
    assert!(text.contains("\nschema_version = 1\n"), "{text}");
    assert!(
        text.contains("\nmicrophone_enabled_by_default = false\n"),
        "{text}"
    );
    assert!(!text.contains("output_directory"), "{text}");
}

#[test]
fn line_numbers_are_one_based_and_clamped() {
    assert_eq!(schema::line_at(b"a\nb\nc", 0), line(1));
    assert_eq!(schema::line_at(b"a\nb\nc", 2), line(2));
    assert_eq!(schema::line_at(b"a\nb\nc", 4), line(3));
    assert_eq!(schema::line_at(b"a\nb\nc", 999), line(3));
}

// -------------------------------------------------------------- real files

#[test]
fn missing_file_loads_defaults_and_creates_nothing() {
    let temp = TempDir::new();
    assert_eq!(temp.store().load(), LoadedSettings::defaults_not_stored());
    assert!(!temp.settings_path().parent().unwrap().exists());
}

#[test]
fn malformed_file_loads_defaults_with_error_and_is_left_unchanged() {
    let temp = TempDir::new();
    let content = b"schema_version = 1\nmicrophone_enabled_by_default = tru\n";
    let path = temp.write_settings(content);
    assert_eq!(
        temp.store().load(),
        rejected(SettingsError::Malformed { line: line(2) })
    );
    assert_eq!(fs::read(path).unwrap(), content);
}

#[test]
fn unsupported_version_file_loads_defaults_with_error_and_is_left_unchanged() {
    let temp = TempDir::new();
    let content = b"schema_version = 2\nnew_feature = true\n";
    let path = temp.write_settings(content);
    assert_eq!(
        temp.store().load(),
        rejected(SettingsError::UnsupportedSchemaVersion {
            found: 2,
            supported: 1
        })
    );
    assert_eq!(fs::read(path).unwrap(), content);
}

#[test]
fn out_of_range_file_loads_defaults_with_error_and_is_left_unchanged() {
    let temp = TempDir::new();
    let content = b"schema_version = 1\nmicrophone_enabled_by_default = true\noutput_directory = \"relative/dir\"\n";
    let path = temp.write_settings(content);
    let loaded = temp.store().load();
    assert_eq!(
        loaded,
        rejected(SettingsError::InvalidOutputDirectory(
            OutputDirectoryViolation::NotAbsolute
        ))
    );
    assert!(
        !loaded.settings.microphone_enabled_by_default,
        "no field of a rejected file is applied"
    );
    assert_eq!(fs::read(path).unwrap(), content);
}

#[test]
fn file_size_limit_is_inclusive_and_oversized_files_are_not_parsed() {
    let temp = TempDir::new();
    let limit = usize::try_from(MAX_SETTINGS_FILE_BYTES).unwrap();
    let mut at_limit = b"schema_version = 1\n#".to_vec();
    at_limit.resize(limit, b'x');
    temp.write_settings(&at_limit);
    assert_eq!(
        temp.store().load(),
        LoadedSettings::stored(RecordingSettings::default())
    );

    at_limit.push(b'x');
    temp.write_settings(&at_limit);
    assert_eq!(
        temp.store().load(),
        rejected(SettingsError::TooLarge {
            limit_bytes: MAX_SETTINGS_FILE_BYTES
        })
    );
}

#[test]
fn non_utf8_file_is_malformed_at_the_offending_line() {
    let temp = TempDir::new();
    temp.write_settings(b"schema_version = 1\n# \xff\n");
    assert_eq!(
        temp.store().load(),
        rejected(SettingsError::Malformed { line: line(2) })
    );
}

#[test]
fn settings_path_that_is_a_directory_is_reported_on_load_and_save() {
    let temp = TempDir::new();
    fs::create_dir_all(temp.settings_path()).unwrap();
    let store = temp.store();
    assert_eq!(
        store.load(),
        rejected(SettingsError::ReadFailed(SettingsIoFailure::NotAFile))
    );
    assert_eq!(
        store.save(&example()),
        Err(SettingsError::WriteFailed(SettingsIoFailure::NotAFile))
    );
    assert!(temp.settings_path().is_dir());
}

#[test]
fn parent_that_is_a_file_is_reported_on_load_and_save() {
    let temp = TempDir::new();
    let parent = temp.settings_path().parent().unwrap().to_path_buf();
    fs::write(&parent, b"not a directory").unwrap();
    let store = temp.store();
    assert_eq!(
        store.load(),
        rejected(SettingsError::ReadFailed(SettingsIoFailure::NotAFile))
    );
    assert_eq!(
        store.save(&example()),
        Err(SettingsError::WriteFailed(SettingsIoFailure::NotAFile))
    );
    assert_eq!(fs::read(parent).unwrap(), b"not a directory");
}

/// A non-regular file at the settings path is rejected from its metadata,
/// before any open. std cannot create a FIFO without FFI or an external
/// tool, so a Unix socket stands in: it exercises the same check, and without
/// it the open would fail with an uncategorized error instead of `NotAFile`.
#[cfg(unix)]
#[test]
fn non_regular_file_is_rejected_before_opening() {
    use std::os::unix::net::UnixListener;

    let temp = TempDir::new();
    let path = temp.settings_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let Ok(_listener) = UnixListener::bind(&path) else {
        eprintln!(
            "skipped non_regular_file_is_rejected_before_opening: \
             cannot create a Unix socket in the temporary directory"
        );
        return;
    };
    assert_eq!(
        temp.store().load(),
        rejected(SettingsError::ReadFailed(SettingsIoFailure::NotAFile))
    );
}

/// An open error other than "not found" (here a file name longer than any
/// Linux file system allows) is a read failure, never "nothing stored". Works
/// for every user, unlike the permission tests below.
#[cfg(unix)]
#[test]
fn unopenable_path_is_a_read_error_not_missing_settings() {
    let temp = TempDir::new();
    let store = TomlSettingsFile::new(temp.0.join("n".repeat(300)).join(SETTINGS_FILE_NAME));
    assert_eq!(
        store.load(),
        rejected(SettingsError::ReadFailed(SettingsIoFailure::Other))
    );
    assert_eq!(
        store.save(&example()),
        Err(SettingsError::WriteFailed(SettingsIoFailure::Other))
    );
}

#[test]
fn save_creates_the_directory_and_round_trips() {
    let temp = TempDir::new();
    let store = temp.store();
    assert_eq!(store.save(&example()), Ok(()));
    assert_eq!(store.load(), LoadedSettings::stored(example()));
    assert_eq!(temp.entries(), vec![SETTINGS_FILE_NAME.to_owned()]);
}

#[test]
fn save_replaces_previous_settings_without_leaving_temporary_files() {
    let temp = TempDir::new();
    let store = temp.store();
    for settings in [example(), RecordingSettings::default(), example()] {
        assert_eq!(store.save(&settings), Ok(()));
        assert_eq!(store.load(), LoadedSettings::stored(settings));
        assert_eq!(temp.entries(), vec![SETTINGS_FILE_NAME.to_owned()]);
    }
}

#[test]
fn read_only_settings_file_is_not_replaced() {
    let temp = TempDir::new();
    let store = temp.store();
    assert_eq!(store.save(&example()), Ok(()));
    let before = fs::read(temp.settings_path()).unwrap();
    read_only(&temp.settings_path());
    assert_eq!(
        store.save(&RecordingSettings::default()),
        Err(SettingsError::WriteFailed(SettingsIoFailure::ReadOnly))
    );
    assert_eq!(fs::read(temp.settings_path()).unwrap(), before);
    assert_eq!(store.load(), LoadedSettings::stored(example()));
    assert_eq!(temp.entries(), vec![SETTINGS_FILE_NAME.to_owned()]);
}

// -------------------------------------------------------- injected faults

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    CreateDir,
    CreateNew,
    WriteAll,
    SyncAll,
    Rename,
    RemoveFile,
}

/// Real filesystem calls, except that the scripted steps fail. A failing
/// `write_all` first writes half of the bytes, like a disk that fills up.
struct FaultyOperations {
    failing: Vec<Step>,
    /// `create_new` reports `AlreadyExists` this many times first.
    stale_names: Mutex<u32>,
}

impl FaultyOperations {
    fn failing(failing: &[Step]) -> Self {
        Self {
            failing: failing.to_vec(),
            stale_names: Mutex::new(0),
        }
    }

    fn with_stale_names(count: u32) -> Self {
        Self {
            failing: Vec::new(),
            stale_names: Mutex::new(count),
        }
    }

    fn check(&self, step: Step) -> io::Result<()> {
        if self.failing.contains(&step) {
            Err(io::Error::new(io::ErrorKind::StorageFull, "injected"))
        } else {
            Ok(())
        }
    }
}

impl FileOperations for FaultyOperations {
    fn create_dir_all(&self, directory: &Path) -> io::Result<()> {
        self.check(Step::CreateDir)?;
        StdFileOperations.create_dir_all(directory)
    }

    fn create_new(&self, path: &Path) -> io::Result<File> {
        self.check(Step::CreateNew)?;
        let mut stale = self.stale_names.lock().unwrap();
        if *stale > 0 {
            *stale -= 1;
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        StdFileOperations.create_new(path)
    }

    fn write_all(&self, file: &mut File, bytes: &[u8]) -> io::Result<()> {
        if self.failing.contains(&Step::WriteAll) {
            let half = bytes.get(..bytes.len() / 2).unwrap_or_default();
            StdFileOperations.write_all(file, half)?;
            return self.check(Step::WriteAll);
        }
        StdFileOperations.write_all(file, bytes)
    }

    fn sync_all(&self, file: &File) -> io::Result<()> {
        self.check(Step::SyncAll)?;
        StdFileOperations.sync_all(file)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.check(Step::Rename)?;
        StdFileOperations.rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.check(Step::RemoveFile)?;
        StdFileOperations.remove_file(path)
    }
}

fn faulty_store(temp: &TempDir, operations: FaultyOperations) -> TomlSettingsFile {
    TomlSettingsFile::with_operations(temp.settings_path(), Box::new(operations))
}

#[test]
fn every_failed_save_step_keeps_previous_settings_and_removes_the_temporary_file() {
    for step in [
        Step::CreateDir,
        Step::CreateNew,
        Step::WriteAll,
        Step::SyncAll,
        Step::Rename,
    ] {
        let temp = TempDir::new();
        assert_eq!(temp.store().save(&example()), Ok(()));
        let before = fs::read(temp.settings_path()).unwrap();

        let store = faulty_store(&temp, FaultyOperations::failing(&[step]));
        assert_eq!(
            store.save(&RecordingSettings::default()),
            Err(SettingsError::WriteFailed(SettingsIoFailure::StorageFull)),
            "{step:?}"
        );
        assert_eq!(
            fs::read(temp.settings_path()).unwrap(),
            before,
            "{step:?} changed the settings file"
        );
        assert_eq!(
            temp.entries(),
            vec![SETTINGS_FILE_NAME.to_owned()],
            "{step:?} left a partial or temporary file"
        );
        assert_eq!(store.load(), LoadedSettings::stored(example()), "{step:?}");
    }
}

#[test]
fn failed_first_save_leaves_no_settings_file() {
    for step in [Step::WriteAll, Step::SyncAll, Step::Rename] {
        let temp = TempDir::new();
        let store = faulty_store(&temp, FaultyOperations::failing(&[step]));
        assert!(store.save(&example()).is_err(), "{step:?}");
        assert!(temp.entries().is_empty(), "{step:?}: {:?}", temp.entries());
        assert_eq!(store.load(), LoadedSettings::defaults_not_stored());
    }
}

#[test]
fn failed_cleanup_leaves_only_an_ignored_temporary_file() {
    let temp = TempDir::new();
    assert_eq!(temp.store().save(&example()), Ok(()));
    let store = faulty_store(
        &temp,
        FaultyOperations::failing(&[Step::Rename, Step::RemoveFile]),
    );
    assert_eq!(
        store.save(&RecordingSettings::default()),
        Err(SettingsError::WriteFailed(SettingsIoFailure::StorageFull))
    );
    let entries = temp.entries();
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert!(entries.iter().any(|name| name == SETTINGS_FILE_NAME));
    assert!(
        entries
            .iter()
            .any(|name| name.starts_with(".settings.toml.") && name.ends_with(".tmp"))
    );
    assert_eq!(temp.store().load(), LoadedSettings::stored(example()));
}

#[test]
fn stale_temporary_names_are_skipped() {
    let temp = TempDir::new();
    let store = faulty_store(&temp, FaultyOperations::with_stale_names(3));
    assert_eq!(store.save(&example()), Ok(()));
    assert_eq!(store.load(), LoadedSettings::stored(example()));
}

#[test]
fn exhausted_temporary_names_fail_without_touching_settings() {
    let temp = TempDir::new();
    assert_eq!(temp.store().save(&example()), Ok(()));
    let store = faulty_store(
        &temp,
        FaultyOperations::with_stale_names(TEMPORARY_NAME_ATTEMPTS),
    );
    assert_eq!(
        store.save(&RecordingSettings::default()),
        Err(SettingsError::WriteFailed(SettingsIoFailure::Other))
    );
    assert_eq!(store.load(), LoadedSettings::stored(example()));
}

#[test]
fn io_errors_map_to_adapter_neutral_failures() {
    for (kind, expected) in [
        (
            io::ErrorKind::PermissionDenied,
            SettingsIoFailure::PermissionDenied,
        ),
        (
            io::ErrorKind::ReadOnlyFilesystem,
            SettingsIoFailure::ReadOnly,
        ),
        (io::ErrorKind::StorageFull, SettingsIoFailure::StorageFull),
        (io::ErrorKind::QuotaExceeded, SettingsIoFailure::StorageFull),
        (io::ErrorKind::NotADirectory, SettingsIoFailure::NotAFile),
        (io::ErrorKind::IsADirectory, SettingsIoFailure::NotAFile),
        (io::ErrorKind::AlreadyExists, SettingsIoFailure::NotAFile),
        (io::ErrorKind::Interrupted, SettingsIoFailure::Other),
        (io::ErrorKind::Other, SettingsIoFailure::Other),
    ] {
        assert_eq!(io_failure(&io::Error::from(kind)), expected, "{kind:?}");
    }
}

#[test]
fn debug_output_names_the_path_only() {
    let store = TomlSettingsFile::new("/x/settings.toml");
    assert_eq!(store.path(), Path::new("/x/settings.toml"));
    assert_eq!(
        format!("{store:?}"),
        "TomlSettingsFile { path: \"/x/settings.toml\", .. }"
    );
}

// ------------------------------------------------- Unix permission checks

/// Whether this process ignores the permission bits just set on `directory`
/// (for example root or `CAP_DAC_OVERRIDE`).
#[cfg(unix)]
fn permissions_are_bypassed(directory: &Path) -> bool {
    let probe = directory.join(".privilege-probe");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(unix)]
#[test]
fn read_only_directory_save_fails_and_keeps_settings() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new();
    let store = temp.store();
    assert_eq!(store.save(&example()), Ok(()));
    let before = fs::read(temp.settings_path()).unwrap();
    let directory = temp.settings_path().parent().unwrap().to_path_buf();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
    if permissions_are_bypassed(&directory) {
        eprintln!(
            "skipped read_only_directory_save_fails_and_keeps_settings: \
             running with privileges that bypass directory permissions (root)"
        );
        return;
    }
    assert_eq!(
        store.save(&RecordingSettings::default()),
        Err(SettingsError::WriteFailed(
            SettingsIoFailure::PermissionDenied
        ))
    );
    assert_eq!(fs::read(temp.settings_path()).unwrap(), before);
    assert_eq!(temp.entries(), vec![SETTINGS_FILE_NAME.to_owned()]);
    assert_eq!(store.load(), LoadedSettings::stored(example()));
}

#[cfg(unix)]
#[test]
fn unreadable_file_loads_defaults_with_error() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new();
    assert_eq!(temp.store().save(&example()), Ok(()));
    fs::set_permissions(temp.settings_path(), fs::Permissions::from_mode(0o000)).unwrap();
    if File::open(temp.settings_path()).is_ok() {
        eprintln!(
            "skipped unreadable_file_loads_defaults_with_error: \
             running with privileges that bypass file permissions (root)"
        );
        return;
    }
    assert_eq!(
        temp.store().load(),
        rejected(SettingsError::ReadFailed(
            SettingsIoFailure::PermissionDenied
        ))
    );
}

#[cfg(unix)]
#[test]
fn saved_file_and_created_directory_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new();
    assert_eq!(temp.store().save(&example()), Ok(()));
    let file_mode = fs::metadata(temp.settings_path())
        .unwrap()
        .permissions()
        .mode();
    let directory_mode = fs::metadata(temp.settings_path().parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(file_mode & 0o077, 0, "file mode {file_mode:o}");
    assert_eq!(
        directory_mode & 0o077,
        0,
        "directory mode {directory_mode:o}"
    );
}

// ---------------------------------------------------------------- location

fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let pairs: Vec<(String, OsString)> = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
        .collect();
    move |name| {
        pairs
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.clone())
    }
}

#[cfg(unix)]
#[test]
fn linux_location_prefers_an_absolute_xdg_config_home() {
    let lookup = environment(&[("XDG_CONFIG_HOME", "/cfg"), ("HOME", "/home/u")]);
    assert_eq!(
        linux_settings_path(lookup),
        Ok(PathBuf::from("/cfg/framelet/settings.toml"))
    );
}

#[cfg(unix)]
#[test]
fn linux_location_falls_back_to_home_config() {
    for xdg in [None, Some(""), Some("relative/cfg")] {
        let mut pairs = vec![("HOME", "/home/u")];
        if let Some(xdg) = xdg {
            pairs.push(("XDG_CONFIG_HOME", xdg));
        }
        assert_eq!(
            linux_settings_path(environment(&pairs)),
            Ok(PathBuf::from("/home/u/.config/framelet/settings.toml")),
            "{xdg:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn linux_location_is_unavailable_without_an_absolute_base() {
    for pairs in [
        vec![],
        vec![("HOME", "")],
        vec![("HOME", "home/u")],
        vec![("XDG_CONFIG_HOME", "cfg"), ("HOME", "")],
    ] {
        assert_eq!(
            linux_settings_path(environment(&pairs)),
            Err(SettingsError::LocationUnavailable),
            "{pairs:?}"
        );
    }
}

#[cfg(windows)]
#[test]
fn windows_location_uses_appdata() {
    let lookup = environment(&[("APPDATA", r"C:\Users\u\AppData\Roaming")]);
    assert_eq!(
        windows_settings_path(lookup),
        Ok(PathBuf::from(
            r"C:\Users\u\AppData\Roaming\Framelet\settings.toml"
        ))
    );
}

#[cfg(windows)]
#[test]
fn windows_location_is_unavailable_without_an_absolute_appdata() {
    for pairs in [
        vec![],
        vec![("APPDATA", "")],
        vec![("APPDATA", r"AppData\Roaming")],
    ] {
        assert_eq!(
            windows_settings_path(environment(&pairs)),
            Err(SettingsError::LocationUnavailable),
            "{pairs:?}"
        );
    }
}

#[test]
fn platform_location_uses_the_platform_rule() {
    #[cfg(windows)]
    let (pairs, expected) = (
        [("APPDATA", r"C:\Roaming"), ("XDG_CONFIG_HOME", r"C:\cfg")],
        r"C:\Roaming\Framelet\settings.toml",
    );
    #[cfg(not(windows))]
    let (pairs, expected) = (
        [("APPDATA", "/roaming"), ("XDG_CONFIG_HOME", "/cfg")],
        "/cfg/framelet/settings.toml",
    );
    assert_eq!(
        platform_settings_path(environment(&pairs)),
        Ok(PathBuf::from(expected))
    );
}

#[test]
fn origin_of_a_stored_file_is_stored() {
    let temp = TempDir::new();
    assert_eq!(temp.store().save(&example()), Ok(()));
    assert_eq!(temp.store().load().origin, SettingsOrigin::Stored);
}
