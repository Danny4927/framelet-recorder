//! Private filesystem seam used by the storage adapter.
//!
//! The seam exists so adapter tests can inject faults into individual
//! filesystem operations. It is not a port: it never crosses the core boundary.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// An open file owned by the storage adapter.
pub(crate) struct FileHandle {
    file: File,
}

/// A directory entry reported by [`FileOperations::read_dir_names`].
pub(crate) struct DirectoryEntry {
    /// File name of the entry, lossily converted for prefix matching.
    pub(crate) name: String,
    /// Full path of the entry.
    pub(crate) path: PathBuf,
    /// Whether the entry is a regular file (symlinks are not followed).
    pub(crate) is_file: bool,
}

/// Filesystem operations the storage adapter performs.
pub(crate) trait FileOperations {
    /// Creates a new file, failing with `AlreadyExists` if the path exists.
    fn create_exclusive(&self, path: &Path) -> io::Result<FileHandle>;
    /// Opens an existing file for reading and writing.
    fn open_existing(&self, path: &Path) -> io::Result<FileHandle>;
    /// Flushes buffered writes.
    fn flush(&self, handle: &mut FileHandle) -> io::Result<()>;
    /// Closes the handle: performs the file's single `sync_all` (data and metadata),
    /// then drops it, so durability and close-time errors surface instead of being lost.
    fn close(&self, handle: FileHandle) -> io::Result<()>;
    /// Creates `destination` as a hard link to `source`; never replaces an existing path.
    fn hard_link_no_replace(&self, source: &Path, destination: &Path) -> io::Result<()>;
    /// Removes a file.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Creates a directory and its parents.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Returns whether the path is a directory.
    fn metadata_is_dir(&self, path: &Path) -> io::Result<bool>;
    /// Lists the entries of a directory.
    fn read_dir_names(&self, path: &Path) -> io::Result<Vec<DirectoryEntry>>;
}

/// [`FileOperations`] backed by `std::fs`.
pub(crate) struct StdFileOperations;

impl FileOperations for StdFileOperations {
    fn create_exclusive(&self, path: &Path) -> io::Result<FileHandle> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(|file| FileHandle { file })
    }

    fn open_existing(&self, path: &Path) -> io::Result<FileHandle> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map(|file| FileHandle { file })
    }

    fn flush(&self, handle: &mut FileHandle) -> io::Result<()> {
        handle.file.flush()
    }

    fn close(&self, handle: FileHandle) -> io::Result<()> {
        let result = handle.file.sync_all();
        drop(handle);
        result
    }

    fn hard_link_no_replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        // Hard-link creation fails with AlreadyExists and never replaces the target.
        fs::hard_link(source, destination)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn metadata_is_dir(&self, path: &Path) -> io::Result<bool> {
        fs::metadata(path).map(|metadata| metadata.is_dir())
    }

    fn read_dir_names(&self, path: &Path) -> io::Result<Vec<DirectoryEntry>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            entries.push(DirectoryEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
                is_file: entry.file_type()?.is_file(),
            });
        }
        Ok(entries)
    }
}
