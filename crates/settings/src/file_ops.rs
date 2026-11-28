//! Filesystem calls made by [`crate::TomlSettingsFile`] when saving.
//!
//! Every fallible call of the atomic write goes through [`FileOperations`] so
//! that tests can inject a failure at each step (`src/tests.rs`). Reading is
//! a single open-and-read and is exercised with real files instead.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// The calls of the atomic write, in the order the adapter makes them.
pub(crate) trait FileOperations: Send + Sync {
    /// Creates `directory` and missing parents (owner-only on Unix).
    fn create_dir_all(&self, directory: &Path) -> io::Result<()>;
    /// Creates `path` exclusively; fails with `AlreadyExists` if it exists.
    fn create_new(&self, path: &Path) -> io::Result<File>;
    /// Writes all of `bytes` to `file`.
    fn write_all(&self, file: &mut File, bytes: &[u8]) -> io::Result<()>;
    /// Flushes `file` to stable storage.
    fn sync_all(&self, file: &File) -> io::Result<()>;
    /// Atomically replaces `to` with `from` (same directory).
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Removes `path`.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
}

/// The real filesystem.
#[derive(Debug, Default)]
pub(crate) struct StdFileOperations;

impl FileOperations for StdFileOperations {
    fn create_dir_all(&self, directory: &Path) -> io::Result<()> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory)
    }

    fn create_new(&self, path: &Path) -> io::Result<File> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)
    }

    fn write_all(&self, file: &mut File, bytes: &[u8]) -> io::Result<()> {
        file.write_all(bytes)
    }

    fn sync_all(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}
