#![forbid(unsafe_code)]
//! Runs the shared `RecordingStoragePort` contract suites (ADR-0021) against
//! `FileSystemRecordingStorage` in a fresh temporary directory per subject.
//!
//! The same suites run against the core's in-memory fake
//! (`framelet_core::contract::tests`), so fake and adapter cannot drift.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use framelet_core::RecordingArtifact;
use framelet_core::contract::storage::{self, StorageSubject};
use framelet_core::ports::{OutputTarget, RecordingStoragePort};
use framelet_storage::FileSystemRecordingStorage;

/// Distinguishes subjects created by one test process.
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// Filesystem adapter plus a directory it owns; the directory is removed on drop.
struct FileSystemSubject {
    directory: PathBuf,
    storage: FileSystemRecordingStorage,
}

impl FileSystemSubject {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "framelet-storage-contract-{}-{}-{nonce}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        let storage = FileSystemRecordingStorage::new(&directory).expect("storage adapter");
        Self { directory, storage }
    }
}

impl Drop for FileSystemSubject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

impl StorageSubject for FileSystemSubject {
    fn storage(&mut self) -> &mut dyn RecordingStoragePort {
        &mut self.storage
    }

    fn write_media(&mut self, target: &OutputTarget, bytes: &[u8]) {
        // Stands in for the media writer: write, then close by dropping.
        fs::write(target.as_path(), bytes).expect("write candidate");
    }

    fn candidate_exists(&self, target: &OutputTarget) -> bool {
        target.as_path().exists()
    }

    fn published_media(&self, artifact: &RecordingArtifact) -> Option<Vec<u8>> {
        fs::read(Path::new(artifact.location())).ok()
    }
}

#[test]
fn filesystem_storage_reservations_are_exclusive() {
    storage::reservations_are_exclusive(FileSystemSubject::new);
}

#[test]
fn filesystem_storage_publishes_closed_output_once() {
    storage::publishes_closed_output_once(FileSystemSubject::new);
}

#[test]
fn filesystem_storage_abort_is_idempotent_and_spares_published_output() {
    storage::abort_is_idempotent_and_spares_published_output(FileSystemSubject::new);
}

#[test]
fn filesystem_storage_rejects_foreign_closed_output() {
    storage::rejects_foreign_closed_output(FileSystemSubject::new);
}

#[test]
fn filesystem_storage_rejects_unknown_session() {
    storage::rejects_unknown_session(FileSystemSubject::new);
}

#[test]
fn filesystem_storage_passes_the_whole_storage_suite() {
    storage::run_all(FileSystemSubject::new);
}
