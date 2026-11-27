//! Fault-injected adapter tests for reservation, publication, abort, and recovery.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use framelet_core::ports::{
    ClosedOutput, PortError, RecordingStoragePort, StorageReservation, StorageSessionId,
};
use framelet_core::{RecordingConfiguration, RecordingId, RecordingMode};

use crate::FileSystemRecordingStorage;
use crate::fault::{Fault, FaultControl, FaultyFileOperations, Operation};

struct Fixture {
    directory: PathBuf,
    control: FaultControl,
    storage: FileSystemRecordingStorage,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "framelet-storage-fault-{label}-{}-{nonce}",
            std::process::id()
        ));
        let control = FaultControl::default();
        let storage = FileSystemRecordingStorage::with_operations(
            &directory,
            Box::new(FaultyFileOperations::new(control.clone())),
        )
        .expect("storage");
        Self {
            directory,
            control,
            storage,
        }
    }

    fn begin(&mut self, id: u64) -> Result<StorageReservation, PortError> {
        let configuration =
            RecordingConfiguration::new(RecordingMode::Screen, false).expect("configuration");
        self.storage.begin(RecordingId::new(id), &configuration)
    }

    fn begin_with_media(&mut self, id: u64, media: &[u8]) -> (StorageReservation, PathBuf) {
        let reservation = self.begin(id).expect("begin");
        let candidate = reservation.target.as_path().to_path_buf();
        fs::write(&candidate, media).expect("write candidate");
        (reservation, candidate)
    }

    fn publish(&mut self, reservation: &StorageReservation) -> Result<String, PortError> {
        self.storage
            .publish(
                &reservation.session,
                ClosedOutput::new(reservation.target.clone()),
            )
            .map(|artifact| artifact.location().to_owned())
    }

    fn entries(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.directory)
            .expect("list")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    fn final_path(&self, id: u64, sequence: u64) -> PathBuf {
        self.directory
            .join(format!("recording-{id}-{sequence}.framelet"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn assert_no_session(storage: &FileSystemRecordingStorage, session: &str) {
    let id = StorageSessionId::new(session.to_owned()).expect("id");
    assert!(!storage.is_active(&id));
}

#[test]
fn reserve_failures_map_storage_full_and_clean_up_temporary_file() {
    for operation in [Operation::Flush, Operation::Close] {
        let mut fixture = Fixture::new("reserve");
        fixture.control.fail_next(operation, Fault::StorageFull);
        assert_eq!(
            fixture.begin(1),
            Err(PortError::StorageExhausted),
            "{operation:?}"
        );
        assert!(fixture.entries().is_empty(), "{operation:?} left files");
        assert_no_session(&fixture.storage, "filesystem-1");
    }
}

#[test]
fn reserve_create_failure_is_mapped_and_registers_nothing() {
    let mut fixture = Fixture::new("reserve-create");
    fixture
        .control
        .fail_next(Operation::CreateExclusive, Fault::PermissionDenied);
    assert!(matches!(fixture.begin(1), Err(PortError::Failed(_))));
    assert!(fixture.entries().is_empty());
    assert_no_session(&fixture.storage, "filesystem-1");
}

#[test]
fn reserve_failure_with_cleanup_failure_reports_both_and_leaves_orphan() {
    let mut fixture = Fixture::new("reserve-cleanup");
    fixture.control.fail_next(Operation::Close, Fault::Other);
    fixture
        .control
        .fail_next(Operation::RemoveFile, Fault::PermissionDenied);
    match fixture.begin(1) {
        Err(PortError::Failed(message)) => {
            assert!(message.contains("injected fault"), "{message}");
            assert!(message.contains("permission denied"), "{message}");
        }
        other => panic!("unexpected result {other:?}"),
    }
    assert_no_session(&fixture.storage, "filesystem-1");
    // The leftover candidate is discoverable, never silently lost.
    assert_eq!(
        fixture
            .storage
            .find_orphaned_temporary_files()
            .expect("scan")
            .len(),
        1
    );
}

#[test]
fn reserve_storage_full_stays_typed_when_cleanup_fails() {
    let mut fixture = Fixture::new("reserve-full-cleanup");
    fixture
        .control
        .fail_next(Operation::Close, Fault::StorageFull);
    fixture
        .control
        .fail_next(Operation::RemoveFile, Fault::PermissionDenied);
    assert_eq!(fixture.begin(1), Err(PortError::StorageExhausted));
    assert_no_session(&fixture.storage, "filesystem-1");
    assert_eq!(
        fixture
            .storage
            .find_orphaned_temporary_files()
            .expect("scan")
            .len(),
        1
    );
}

#[test]
fn reserve_handle_is_closed_before_cleanup() {
    let mut fixture = Fixture::new("reserve-order");
    fixture.control.fail_next(Operation::Flush, Fault::Other);
    assert!(fixture.begin(1).is_err());
    let calls = fixture.control.calls();
    assert_eq!(
        &calls[calls.len() - 3..],
        &[
            Operation::CreateExclusive,
            Operation::Flush,
            Operation::RemoveFile
        ]
    );
}

#[test]
fn publish_settle_failures_publish_nothing_and_session_stays_abortable() {
    for operation in [Operation::OpenExisting, Operation::Close] {
        let mut fixture = Fixture::new("publish-settle");
        let (reservation, candidate) = fixture.begin_with_media(3, b"media");
        fixture.control.fail_next(operation, Fault::StorageFull);
        assert_eq!(
            fixture.publish(&reservation),
            Err(PortError::StorageExhausted),
            "{operation:?}"
        );
        assert!(!fixture.final_path(3, 1).exists(), "{operation:?}");
        assert_eq!(fs::read(&candidate).expect("retained"), b"media");
        fixture.storage.abort(&reservation.session).expect("abort");
        assert!(fixture.entries().is_empty());
    }
}

#[test]
fn publish_link_failure_leaves_existing_destination_untouched() {
    let mut fixture = Fixture::new("publish-link");
    let existing = fixture.final_path(4, 1);
    fs::write(&existing, b"keep").expect("seed");
    let (reservation, _) = fixture.begin_with_media(4, b"new");
    fixture
        .control
        .fail_next(Operation::HardLink, Fault::PermissionDenied);
    assert!(fixture.publish(&reservation).is_err());
    assert_eq!(fs::read(&existing).expect("existing"), b"keep");
    fixture.storage.abort(&reservation.session).expect("abort");
    assert_eq!(fixture.entries(), vec!["recording-4-1.framelet".to_owned()]);
}

#[test]
fn publish_link_failure_without_destination_publishes_nothing() {
    let mut fixture = Fixture::new("publish-link-empty");
    let (reservation, candidate) = fixture.begin_with_media(5, b"media");
    fixture
        .control
        .fail_next(Operation::HardLink, Fault::StorageFull);
    assert_eq!(
        fixture.publish(&reservation),
        Err(PortError::StorageExhausted)
    );
    assert!(!fixture.final_path(5, 1).exists());
    assert!(candidate.exists());
    fixture.storage.abort(&reservation.session).expect("abort");
}

#[test]
fn publish_cleanup_failure_reports_error_keeps_complete_file_and_never_overwrites() {
    let mut fixture = Fixture::new("publish-cleanup");
    let (reservation, candidate) = fixture.begin_with_media(6, b"complete");
    fixture
        .control
        .fail_next(Operation::RemoveFile, Fault::PermissionDenied);
    assert!(fixture.publish(&reservation).is_err());
    let published = fixture.final_path(6, 1);
    assert_eq!(fs::read(&published).expect("published"), b"complete");

    // A retry must not overwrite the visible recording.
    fs::write(&candidate, b"changed").expect("mutate candidate");
    assert!(fixture.publish(&reservation).is_err());
    // Hard links share data; the published name still exists and was not replaced.
    assert!(published.exists());
    assert_eq!(
        fixture
            .control
            .calls()
            .iter()
            .filter(|call| **call == Operation::HardLink)
            .count(),
        2
    );

    // Cleanup can be retried through abort, which leaves the published name.
    fixture.storage.abort(&reservation.session).expect("abort");
    assert!(published.exists());
    assert!(!candidate.exists());
}

#[test]
fn abort_remove_failure_keeps_session_for_retry() {
    let mut fixture = Fixture::new("abort");
    let (reservation, candidate) = fixture.begin_with_media(7, b"partial");
    fixture
        .control
        .fail_next(Operation::RemoveFile, Fault::PermissionDenied);
    assert!(fixture.storage.abort(&reservation.session).is_err());
    assert!(candidate.exists());
    assert!(fixture.storage.is_active(&reservation.session));
    fixture
        .storage
        .abort(&reservation.session)
        .expect("retry abort");
    assert!(!candidate.exists());
}

#[test]
fn orphan_scan_read_dir_failure_deletes_nothing() {
    let fixture = Fixture::new("orphan");
    let orphan = fixture
        .directory
        .join(".framelet-pending-recording-8-1.part");
    fs::write(&orphan, b"recoverable").expect("seed");
    fixture.control.fail_next(Operation::ReadDir, Fault::Other);
    assert!(fixture.storage.find_orphaned_temporary_files().is_err());
    assert_eq!(fs::read(&orphan).expect("orphan"), b"recoverable");
    assert!(!fixture.control.calls().contains(&Operation::RemoveFile));
}

#[test]
fn constructor_failures_are_typed() {
    let control = FaultControl::default();
    control.fail(Operation::CreateDirAll, 1, Fault::StorageFull);
    let result = FileSystemRecordingStorage::with_operations(
        Path::new("unused-never-created"),
        Box::new(FaultyFileOperations::new(control)),
    );
    assert!(matches!(result, Err(PortError::StorageExhausted)));
}

#[test]
fn storage_full_at_any_step_never_yields_success() {
    let cases: &[(&str, Operation, usize)] = &[
        ("reserve create", Operation::CreateExclusive, 1),
        ("reserve flush", Operation::Flush, 1),
        ("reserve close", Operation::Close, 1),
        ("publish open", Operation::OpenExisting, 1),
        ("publish close", Operation::Close, 2),
        ("publish link", Operation::HardLink, 1),
        ("publish cleanup", Operation::RemoveFile, 1),
    ];
    for (label, operation, nth) in cases {
        let mut fixture = Fixture::new("table");
        fixture.control.fail(*operation, *nth, Fault::StorageFull);
        let outcome = fixture.begin(9).and_then(|reservation| {
            fs::write(reservation.target.as_path(), b"media").expect("write candidate");
            fixture.publish(&reservation)
        });
        assert_eq!(outcome, Err(PortError::StorageExhausted), "{label}");
        fixture.control.clear();
    }
}

#[test]
fn not_found_candidate_is_reported_as_failure() {
    let mut fixture = Fixture::new("not-found");
    let (reservation, _) = fixture.begin_with_media(10, b"media");
    fixture
        .control
        .fail_next(Operation::OpenExisting, Fault::NotFound);
    assert!(matches!(
        fixture.publish(&reservation),
        Err(PortError::Failed(_))
    ));
    fixture.storage.abort(&reservation.session).expect("abort");
}

#[test]
fn repeated_abort_and_abort_after_publish_touch_no_file() {
    let mut fixture = Fixture::new("abort-idempotent");
    let (aborted, candidate) = fixture.begin_with_media(11, b"partial");
    fixture.storage.abort(&aborted.session).expect("abort");
    assert!(!candidate.exists());
    let (published, _) = fixture.begin_with_media(12, b"complete");
    let location = fixture.publish(&published).expect("publish");

    let calls_before = fixture.control.calls().len();
    fixture
        .storage
        .abort(&aborted.session)
        .expect("second abort");
    fixture
        .storage
        .abort(&published.session)
        .expect("abort after publish");
    assert_eq!(
        fixture.control.calls().len(),
        calls_before,
        "an inactive session must not reach the filesystem"
    );
    assert_eq!(fs::read(location).expect("published"), b"complete");
}

#[test]
fn foreign_closed_output_is_rejected_before_any_filesystem_call() {
    let mut fixture = Fixture::new("foreign");
    let (own, own_candidate) = fixture.begin_with_media(13, b"own");
    let (other, other_candidate) = fixture.begin_with_media(14, b"other");
    let calls_before = fixture.control.calls().len();
    let result = fixture
        .storage
        .publish(&own.session, ClosedOutput::new(other.target.clone()));
    assert!(
        matches!(result, Err(PortError::InvalidInput(_))),
        "{result:?}"
    );
    assert_eq!(fixture.control.calls().len(), calls_before);
    assert!(own_candidate.exists() && other_candidate.exists());
    assert!(!fixture.final_path(13, 1).exists());
    assert!(!fixture.final_path(14, 2).exists());
    // The session is unaffected and still publishes its own output.
    assert!(fixture.publish(&own).is_ok());
}
