# ADR-0014: Transactional filesystem recording storage

- **Date:** 17 November 2026
- **Status:** Accepted for the Phase 3 storage adapter slice
- **Related:** ADR-0001, ADR-0002, ADR-0010, ADR-0011, ADR-0012, ADR-0013

## Context

The Recording bounded context already owns a `RecordingStoragePort`, but no
filesystem adapter exists. Recording output must not appear complete while a media
writer is active, and existing recordings must never be overwritten. Process
interruption can leave temporary output that requires explicit handling. The
production media runtime remains unselected under ADR-0004.

## Decision

Add a standalone `framelet-storage` outbound adapter depending inward on
`framelet-core`. It uses safe Rust standard-library filesystem operations only
and introduces no third-party dependency.

- `begin` exclusively creates a uniquely named `.part` candidate.
- The media adapter obtains the temporary path and writes output there.
- `publish` is called only after the media pipeline drains and closes its writer.
  It synchronizes the candidate, creates the final name through a same-filesystem
  hard link (atomic visibility and no-replace semantics), then removes the temporary
  name.
- Filesystems without hard-link support fail closed; no unsafe or overwrite-capable
  fallback is permitted.
- `abort` removes only the temporary candidate owned by the active session.
- An interruption scan reports orphaned candidates but never deletes them.
  Automatic recovery awaits a selected container format and media validation.

## Alternatives

- Write directly to the final path: rejected because partial output becomes visible.
- Rename-overwrite: rejected because it may replace an existing recording.
- Delete all temporary files at startup: rejected because this destroys potentially
  recoverable media without user intent.
- Add a media/container dependency now: rejected because Phase 2 has not selected
  a production runtime or output format.

## Consequences

- Storage is independent of codec, muxer, device, UI, and OS capture APIs.
- No new external dependency or first-party unsafe code is introduced.
- Hard-link publication is atomic and does not replace a target; unsupported
  filesystems fail explicitly.
- Full crash-consistency guarantees, directory durability, automatic recovery,
  metadata journaling, and platform-specific fault injection remain open.
- The desktop composition root remains unchanged until compatible media wiring.

## Fault-injection seam

All filesystem calls go through a crate-private `FileOperations` trait
(`crates/storage/src/fs_ops.rs`). Production uses `StdFileOperations` (`std::fs`);
tests use a `#[cfg(test)]` `FaultyFileOperations` that wraps it on a temporary
directory and fails scripted calls. The seam is not a core port, never crosses
the core boundary, and leaves the public `FileSystemRecordingStorage::new` API
unchanged.

`close` performs the file's single `sync_all` (there is no separate sync step)
and then drops the handle, so durability and close-time errors surface instead
of being lost in `Drop`. Reservation runs create-exclusive, flush, close;
publication runs open, close,
hard-link, remove-temporary. A handle is always released before cleanup.

| Failure | Result | Filesystem state |
| --- | --- | --- |
| `StorageFull` at any step | `PortError::StorageExhausted` | as below |
| `PermissionDenied`, `NotFound`, `AlreadyExists`, other | `PortError::Failed(..)` | as below |
| Reservation flush/close fails | error, no session registered | candidate removed |
| ...and cleanup also fails | `StorageExhausted` if the cause was `StorageFull`, else `Failed` naming both causes | candidate left, reported by orphan scan |
| Publication open/close/link fails | error, session retained | no final file; existing destination untouched; `abort` works |
| Link succeeds, temporary removal fails | error, session retained | complete final file visible; retry refuses to overwrite; `abort` removes the candidate |
| Abort removal fails | error, session retained | candidate kept; abort retryable |
| Orphan scan fails | error | nothing deleted |

## Validation

- Unit tests cover exclusive temporary output, successful publication, abort,
  preservation of existing files, missing-candidate failure, and orphan scanning.
- Fault-injected adapter tests cover the failure table above, including a
  table-driven check that `StorageFull` at any step never yields success.
- Linux and Windows CI check compilation, tests, Clippy, and formatting.
- Real power-loss and clean-machine filesystem validation remain release gates.
