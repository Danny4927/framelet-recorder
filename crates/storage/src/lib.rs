#![forbid(unsafe_code)]
//! Transactional local-file storage adapter for the Recording bounded context.
//!
//! The adapter reserves an exclusive temporary file, accepts media written by a
//! separate media adapter, and publishes only after that writer has closed.

mod fs_ops;

#[cfg(test)]
mod fault;

#[cfg(test)]
mod fault_tests;

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use framelet_core::ports::{MediaSessionId, PortError, RecordingStoragePort};
use framelet_core::{RecordingArtifact, RecordingConfiguration, RecordingId};

use fs_ops::{FileHandle, FileOperations, StdFileOperations};

/// Filesystem-backed implementation of the core-owned storage port.
///
/// Calls belong on an application worker, never on the UI or capture callback.
/// One instance owns its active session map and requires exclusive mutable access.
pub struct FileSystemRecordingStorage {
    output_directory: PathBuf,
    active: HashMap<MediaSessionId, PendingOutput>,
    next_session: u64,
    operations: Box<dyn FileOperations + Send>,
}

struct PendingOutput {
    temporary_path: PathBuf,
    final_path: PathBuf,
}

impl FileSystemRecordingStorage {
    /// Creates and validates the output directory.
    pub fn new(output_directory: impl Into<PathBuf>) -> Result<Self, PortError> {
        Self::with_operations(output_directory, Box::new(StdFileOperations))
    }

    /// Creates the adapter with an explicit filesystem seam (used by fault-injection tests).
    pub(crate) fn with_operations(
        output_directory: impl Into<PathBuf>,
        operations: Box<dyn FileOperations + Send>,
    ) -> Result<Self, PortError> {
        let output_directory = output_directory.into();
        operations
            .create_dir_all(&output_directory)
            .map_err(map_io_error)?;
        if !operations
            .metadata_is_dir(&output_directory)
            .map_err(map_io_error)?
        {
            return Err(PortError::InvalidInput(
                "recording output path is not a directory".into(),
            ));
        }
        Ok(Self {
            output_directory,
            active: HashMap::new(),
            next_session: 1,
            operations,
        })
    }

    /// Returns the temporary candidate path for a prepared session.
    ///
    /// The media adapter writes to this path and must close its writer before publish.
    pub fn temporary_path(&self, session: &MediaSessionId) -> Result<&Path, PortError> {
        self.active
            .get(session)
            .map(|pending| pending.temporary_path.as_path())
            .ok_or_else(|| PortError::InvalidInput("unknown storage session".into()))
    }

    /// Finds interrupted-session candidates without deleting or modifying them.
    pub fn find_orphaned_temporary_files(&self) -> Result<Vec<PathBuf>, PortError> {
        let mut orphaned: Vec<PathBuf> = self
            .operations
            .read_dir_names(&self.output_directory)
            .map_err(map_io_error)?
            .into_iter()
            .filter(|entry| entry.is_file && entry.name.starts_with(".framelet-pending-"))
            .map(|entry| entry.path)
            .collect();
        orphaned.sort();
        Ok(orphaned)
    }

    fn paths(&self, recording_id: RecordingId, sequence: u64) -> (PathBuf, PathBuf) {
        let stem = format!("recording-{}-{sequence}", recording_id.value());
        (
            self.output_directory
                .join(format!(".framelet-pending-{stem}.part")),
            self.output_directory.join(format!("{stem}.framelet")),
        )
    }

    fn reserve_candidate(
        &self,
        recording_id: RecordingId,
        first_sequence: u64,
    ) -> Result<(u64, PathBuf, PathBuf), PortError> {
        let mut sequence = first_sequence;
        loop {
            let (temporary_path, final_path) = self.paths(recording_id, sequence);
            match self.operations.create_exclusive(&temporary_path) {
                Ok(reservation) => {
                    if let Err(error) = self.settle_reservation(reservation) {
                        return Err(match self.operations.remove_file(&temporary_path) {
                            Ok(()) => map_io_error(error),
                            Err(_) if error.kind() == io::ErrorKind::StorageFull => {
                                // Disk-full stays typed; the leftover candidate is
                                // reported by the orphan scan.
                                PortError::StorageExhausted
                            }
                            Err(cleanup_error) => PortError::Failed(format!(
                                "could not persist temporary recording ({}); cleanup also failed ({})",
                                describe(&error),
                                describe(&cleanup_error)
                            )),
                        });
                    }
                    return Ok((sequence, temporary_path, final_path));
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    sequence = sequence.checked_add(1).ok_or_else(|| {
                        PortError::Failed("storage session identity exhausted".into())
                    })?;
                }
                Err(error) => return Err(map_io_error(error)),
            }
        }
    }

    /// Flushes and explicitly closes (close syncs) a handle so every error surfaces.
    /// The handle is always released, even when an earlier step fails.
    fn settle_reservation(&self, mut handle: FileHandle) -> io::Result<()> {
        self.operations.flush(&mut handle)?;
        // `close` performs the single sync_all so its error surfaces.
        self.operations.close(handle)
    }

    fn settle_candidate(&self, handle: FileHandle) -> io::Result<()> {
        // `close` performs the single sync_all so its error surfaces.
        self.operations.close(handle)
    }
}

impl RecordingStoragePort for FileSystemRecordingStorage {
    fn begin(
        &mut self,
        recording_id: RecordingId,
        _configuration: &RecordingConfiguration,
    ) -> Result<MediaSessionId, PortError> {
        let sequence = self.next_session;
        let session = MediaSessionId::new(format!("filesystem-{sequence}"))?;
        let (reserved_sequence, temporary_path, final_path) =
            self.reserve_candidate(recording_id, sequence)?;
        self.next_session = reserved_sequence
            .checked_add(1)
            .ok_or_else(|| PortError::Failed("storage session identity exhausted".into()))?;

        self.active.insert(
            session.clone(),
            PendingOutput {
                temporary_path,
                final_path,
            },
        );
        Ok(session)
    }

    fn publish(&mut self, session: &MediaSessionId) -> Result<RecordingArtifact, PortError> {
        let pending = self
            .active
            .get(session)
            .ok_or_else(|| PortError::InvalidInput("unknown storage session".into()))?;

        // The media writer must be closed by its owner before this call.
        let candidate = self
            .operations
            .open_existing(&pending.temporary_path)
            .map_err(map_io_error)?;
        self.settle_candidate(candidate).map_err(map_io_error)?;

        // Same-filesystem hard-link creation gives atomic visibility and no-replace.
        // It fails closed on filesystems without hard-link support.
        self.operations
            .hard_link_no_replace(&pending.temporary_path, &pending.final_path)
            .map_err(map_io_error)?;

        // Construct the application-owned descriptor before committing the
        // publication. This keeps all fallible validation ahead of temp cleanup.
        let final_path = pending.final_path.clone();
        let artifact = RecordingArtifact::new(final_path.to_string_lossy().into_owned())
            .map_err(|error| PortError::Failed(error.to_string()))?;

        // The destination is now published. A cleanup failure must not turn a
        // successful publication into an ambiguous retry that could report failure
        // while leaving a complete recording visible. Retain the active entry so
        // the caller can retry cleanup; subsequent publish detects the existing
        // destination and refuses to overwrite it.
        self.operations
            .remove_file(&pending.temporary_path)
            .map_err(map_io_error)?;
        self.active.remove(session);
        Ok(artifact)
    }

    fn abort(&mut self, session: &MediaSessionId) -> Result<(), PortError> {
        let pending = self
            .active
            .get(session)
            .ok_or_else(|| PortError::InvalidInput("unknown storage session".into()))?;
        match self.operations.remove_file(&pending.temporary_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(map_io_error(error)),
        }
        self.active.remove(session);
        Ok(())
    }
}

fn describe(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::StorageFull => "storage full".into(),
        io::ErrorKind::PermissionDenied => "permission denied".into(),
        io::ErrorKind::NotFound => "path not found".into(),
        _ => error.to_string(),
    }
}

fn map_io_error(error: io::Error) -> PortError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => PortError::Failed("filesystem permission denied".into()),
        io::ErrorKind::StorageFull => PortError::StorageExhausted,
        io::ErrorKind::AlreadyExists => {
            PortError::Failed("recording destination already exists".into())
        }
        io::ErrorKind::NotFound => PortError::Failed("recording path no longer exists".into()),
        _ => PortError::Failed(format!("filesystem operation failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "framelet-storage-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    fn configuration() -> RecordingConfiguration {
        RecordingConfiguration::new(framelet_core::RecordingMode::Screen, false)
            .expect("valid configuration")
    }

    #[test]
    fn exclusive_candidate_is_published_without_overwrite() {
        let directory = temp_dir("publish");
        let mut storage = FileSystemRecordingStorage::new(&directory).expect("storage");
        let session = storage
            .begin(RecordingId::new(7), &configuration())
            .expect("begin");
        let candidate = storage
            .temporary_path(&session)
            .expect("candidate")
            .to_path_buf();
        fs::write(&candidate, b"complete media").expect("write candidate");
        let artifact = storage.publish(&session).expect("publish");
        let final_path = PathBuf::from(artifact.location());
        assert_eq!(
            fs::read(final_path).expect("read published"),
            b"complete media"
        );
        assert!(!candidate.exists());
        assert!(storage.publish(&session).is_err());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn begin_retries_colliding_temporary_names_without_overwriting() {
        let directory = temp_dir("begin-collision");
        let mut storage = FileSystemRecordingStorage::new(&directory).expect("storage");
        let first = storage
            .begin(RecordingId::new(12), &configuration())
            .expect("first begin");
        let first_candidate = storage
            .temporary_path(&first)
            .expect("first candidate")
            .to_path_buf();

        // A stale candidate from a prior process may occupy the next deterministic
        // name. The adapter must reserve a fresh name rather than truncate it.
        let second_collision = directory.join(".framelet-pending-recording-12-2.part");
        fs::write(&second_collision, b"orphan").expect("seed collision");
        let second = storage
            .begin(RecordingId::new(12), &configuration())
            .expect("retry begin");
        let second_candidate = storage
            .temporary_path(&second)
            .expect("second candidate")
            .to_path_buf();

        assert_ne!(first_candidate, second_candidate);
        assert_ne!(second_candidate, second_collision);
        assert_eq!(fs::read(&second_collision).expect("read orphan"), b"orphan");
        storage.abort(&first).expect("abort first");
        storage.abort(&second).expect("abort second");
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn publication_collision_never_replaces_existing_recording() {
        let directory = temp_dir("publish-collision");
        let existing = directory.join("recording-13-1.framelet");
        fs::write(&existing, b"keep me").expect("seed existing file");
        let mut storage = FileSystemRecordingStorage::new(&directory).expect("storage");
        let session = storage
            .begin(RecordingId::new(13), &configuration())
            .expect("begin");
        let candidate = storage
            .temporary_path(&session)
            .expect("candidate")
            .to_path_buf();
        fs::write(&candidate, b"new media").expect("write candidate");

        assert!(storage.publish(&session).is_err());
        assert_eq!(fs::read(&existing).expect("read existing"), b"keep me");
        assert_eq!(
            fs::read(&candidate).expect("candidate retained"),
            b"new media"
        );
        storage.abort(&session).expect("abort");
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn abort_is_idempotent_when_candidate_was_already_removed() {
        let directory = temp_dir("abort-missing");
        let mut storage = FileSystemRecordingStorage::new(&directory).expect("storage");
        let session = storage
            .begin(RecordingId::new(14), &configuration())
            .expect("begin");
        let candidate = storage
            .temporary_path(&session)
            .expect("candidate")
            .to_path_buf();
        fs::remove_file(candidate).expect("remove candidate");

        storage.abort(&session).expect("abort missing candidate");
        assert!(storage.abort(&session).is_err());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn abort_removes_only_candidate_and_preserves_existing_recording() {
        let directory = temp_dir("abort");
        let existing = directory.join("recording-9-1.framelet");
        fs::write(&existing, b"existing").expect("seed existing file");
        let mut storage = FileSystemRecordingStorage::new(&directory).expect("storage");
        let session = storage
            .begin(RecordingId::new(9), &configuration())
            .expect("begin");
        let candidate = storage
            .temporary_path(&session)
            .expect("candidate")
            .to_path_buf();
        fs::write(&candidate, b"incomplete").expect("write partial");
        storage.abort(&session).expect("abort");
        assert!(!candidate.exists());
        assert_eq!(fs::read(existing).expect("read existing"), b"existing");
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn orphan_scan_reports_but_does_not_delete_interrupted_output() {
        let directory = temp_dir("recovery");
        let orphan = directory.join(".framelet-pending-recording-10-1.part");
        fs::write(&orphan, b"recoverable").expect("seed orphan");
        let storage = FileSystemRecordingStorage::new(&directory).expect("storage");
        assert_eq!(
            storage.find_orphaned_temporary_files().expect("scan"),
            vec![orphan.clone()]
        );
        assert_eq!(fs::read(&orphan).expect("orphan retained"), b"recoverable");
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn missing_candidate_cannot_be_published() {
        let directory = temp_dir("missing");
        let mut storage = FileSystemRecordingStorage::new(&directory).expect("storage");
        let session = storage
            .begin(RecordingId::new(11), &configuration())
            .expect("begin");
        let candidate = storage
            .temporary_path(&session)
            .expect("candidate")
            .to_path_buf();
        fs::remove_file(candidate).expect("remove candidate");
        assert!(storage.publish(&session).is_err());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn output_path_must_be_a_directory() {
        let directory = temp_dir("not-directory");
        let file = directory.join("file");
        fs::write(&file, b"x").expect("create file");
        assert!(FileSystemRecordingStorage::new(file).is_err());
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
