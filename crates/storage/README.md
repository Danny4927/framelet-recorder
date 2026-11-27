# Transactional filesystem storage adapter

This crate implements the core-owned `RecordingStoragePort`. It is an outbound
adapter, not part of the domain model, and does not choose a media runtime, codec,
or container.

## Lifecycle

1. `begin` reserves a unique temporary `.part` file using exclusive creation. If a stale candidate occupies the next generated name, the adapter advances the sequence and retries without truncating the existing file.
2. The media adapter writes its output to `temporary_path(session)`.
3. The media pipeline must drain and close its writer before `publish`.
4. `publish` synchronizes the closed candidate and creates the final name with a
   same-filesystem hard link. This gives atomic visibility and fails if the target
   already exists; it never overwrites an existing recording.
5. `abort` removes only the temporary candidate owned by that session.

Filesystems without hard-link support fail closed. There is no non-atomic fallback.

## Recovery

The adapter reports `.framelet-pending-*.part` files left by interrupted processes.
It never deletes them automatically. Automatic recovery/finalization is deferred
until a production media-container format and its validation contract are selected.

## Failure handling

All filesystem calls go through the private `FileOperations` seam (`src/fs_ops.rs`);
tests inject faults through `src/fault.rs`. `close` performs each file's single `sync_all` and then drops the handle,
so sync and close errors surface. `StorageFull` maps to `PortError::StorageExhausted`; other kinds
map to `PortError::Failed`.

| Failure | Outcome |
| --- | --- |
| `begin` flush/close fails | error; candidate removed; no session |
| ...and cleanup fails | `StorageExhausted` if the cause was disk-full, else `Failed` naming both causes; candidate reported by orphan scan |
| `publish` open/close/link fails | error; no final file; existing destination untouched; session abortable |
| link ok, temp removal fails | error; complete final file visible; retry never overwrites; `abort` cleans up |
| `abort` removal fails | error; candidate kept; session retained for retry |
| orphan scan fails | error; nothing deleted |

## Limitations

- The temporary path is a hand-off seam for the media adapter; `publish` must not
  be called until all media writers are closed.
- Directory fsync and power-loss durability semantics are platform/filesystem
  dependent and are not yet claimed.
- Hosted CI does not replace real-filesystem (NTFS, ext4, btrfs), genuine
  disk-full, or power-loss validation. Injected failures are covered at the
  adapter level only (see [Failure handling](#failure-handling)).
- The desktop composition root is not wired to this adapter yet; the media backend
  decision and later composition phase must provide a compatible pipeline.
