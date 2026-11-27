//! Test-only fault-injecting filesystem seam.
//!
//! Wraps [`StdFileOperations`] so real files are created on a temporary
//! directory, while scripted operations fail with chosen error kinds.

use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::fs_ops::{DirectoryEntry, FileHandle, FileOperations, StdFileOperations};

/// A filesystem operation observable through the seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operation {
    CreateExclusive,
    OpenExisting,
    Flush,
    Close,
    HardLink,
    RemoveFile,
    CreateDirAll,
    MetadataIsDir,
    ReadDir,
}

/// Error kinds the fault script can inject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    StorageFull,
    PermissionDenied,
    NotFound,
    Other,
}

impl Fault {
    fn to_error(self) -> io::Error {
        let kind = match self {
            Self::StorageFull => io::ErrorKind::StorageFull,
            Self::PermissionDenied => io::ErrorKind::PermissionDenied,
            Self::NotFound => io::ErrorKind::NotFound,
            Self::Other => io::ErrorKind::Other,
        };
        io::Error::new(kind, "injected fault")
    }
}

#[derive(Default)]
struct FaultState {
    /// (operation, 1-based call number of that operation, fault).
    script: Vec<(Operation, usize, Fault)>,
    calls: Vec<Operation>,
}

/// Shared view of the fault state, kept by the test after boxing the seam.
#[derive(Clone, Default)]
pub(crate) struct FaultControl {
    state: Arc<Mutex<FaultState>>,
}

impl FaultControl {
    fn lock(&self) -> MutexGuard<'_, FaultState> {
        // A poisoned lock only follows a panicking test; recover the data.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Fails the `nth` call overall (1-based, counted from creation) of `operation`.
    pub(crate) fn fail(&self, operation: Operation, nth: usize, fault: Fault) {
        self.lock().script.push((operation, nth, fault));
    }

    /// Fails the next call of `operation`.
    pub(crate) fn fail_next(&self, operation: Operation, fault: Fault) {
        let mut state = self.lock();
        let nth = state
            .calls
            .iter()
            .filter(|call| **call == operation)
            .count()
            + 1;
        state.script.push((operation, nth, fault));
    }

    /// Removes all scripted faults.
    pub(crate) fn clear(&self) {
        self.lock().script.clear();
    }

    /// Returns the recorded call order.
    pub(crate) fn calls(&self) -> Vec<Operation> {
        self.lock().calls.clone()
    }

    fn record(&self, operation: Operation) -> io::Result<()> {
        let mut state = self.lock();
        state.calls.push(operation);
        let nth = state
            .calls
            .iter()
            .filter(|call| **call == operation)
            .count();
        match state
            .script
            .iter()
            .find(|(op, at, _)| *op == operation && *at == nth)
        {
            Some((_, _, fault)) => Err(fault.to_error()),
            None => Ok(()),
        }
    }
}

/// Fault-injecting [`FileOperations`] implementation.
pub(crate) struct FaultyFileOperations {
    inner: StdFileOperations,
    control: FaultControl,
}

impl FaultyFileOperations {
    pub(crate) fn new(control: FaultControl) -> Self {
        Self {
            inner: StdFileOperations,
            control,
        }
    }
}

impl FileOperations for FaultyFileOperations {
    fn create_exclusive(&self, path: &Path) -> io::Result<FileHandle> {
        self.control.record(Operation::CreateExclusive)?;
        self.inner.create_exclusive(path)
    }

    fn open_existing(&self, path: &Path) -> io::Result<FileHandle> {
        self.control.record(Operation::OpenExisting)?;
        self.inner.open_existing(path)
    }

    fn flush(&self, handle: &mut FileHandle) -> io::Result<()> {
        self.control.record(Operation::Flush)?;
        self.inner.flush(handle)
    }

    fn close(&self, handle: FileHandle) -> io::Result<()> {
        // The handle is released even when the close is scripted to fail.
        let injected = self.control.record(Operation::Close);
        let closed = self.inner.close(handle);
        injected.and(closed)
    }

    fn hard_link_no_replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        self.control.record(Operation::HardLink)?;
        self.inner.hard_link_no_replace(source, destination)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.control.record(Operation::RemoveFile)?;
        self.inner.remove_file(path)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.control.record(Operation::CreateDirAll)?;
        self.inner.create_dir_all(path)
    }

    fn metadata_is_dir(&self, path: &Path) -> io::Result<bool> {
        self.control.record(Operation::MetadataIsDir)?;
        self.inner.metadata_is_dir(path)
    }

    fn read_dir_names(&self, path: &Path) -> io::Result<Vec<DirectoryEntry>> {
        self.control.record(Operation::ReadDir)?;
        self.inner.read_dir_names(path)
    }
}
