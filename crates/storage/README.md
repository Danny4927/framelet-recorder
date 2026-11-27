# Transactional filesystem storage adapter

This crate implements the core-owned `RecordingStoragePort`. It is an outbound
adapter, not part of the domain model, and does not choose a media runtime, codec,
or container.

## Lifecycle

1. `begin` reserves a unique temporary `.part` file using exclusive creation. If a stale candidate occupies the next generated name, the adapter advances the sequence and retries without truncating the existing file.
   It returns a `StorageReservation` whose `OutputTarget` is the candidate path.
2. The core forwards the target to the media pipeline's `prepare`; the media adapter
   writes there. Nobody else asks this adapter for paths (ADR-0021).
3. `publish(session, ClosedOutput)` requires the closed output returned by the media
   pipeline's `close`. A closed output for any other target, or an inactive session,
   is rejected with `InvalidInput` before any filesystem call.
4. `publish` synchronizes the closed candidate and creates the final name with a
   same-filesystem hard link. This gives atomic visibility and fails if the target
   already exists; it never overwrites an existing recording.
5. `abort` removes only the temporary candidate owned by that session. It is
   idempotent: aborting an inactive session (already aborted, published, or unknown)
   succeeds without a filesystem call and never touches a published recording.

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
| `publish` with a foreign `ClosedOutput` | `InvalidInput`; no filesystem call; both reservations intact |
| orphan scan fails | error; nothing deleted |

## Contract suites

`tests/contract.rs` runs every shared `RecordingStoragePort` suite from
`framelet_core::contract::storage` against this adapter in a temporary directory;
the core's in-memory fake passes the same suites. The `contract-tests` feature is
enabled only in `[dev-dependencies]`.

## Limitations

- Publish-after-close is enforced by contract plus target check, not by the type
  alone: `ClosedOutput::new` is public, so a media adapter that claims closure
  without closing its writer is not prevented (see ADR-0021).
- Directory fsync and power-loss durability semantics are platform/filesystem
  dependent and are not yet claimed.
- Hosted CI does not replace real-filesystem (NTFS, ext4, btrfs), genuine
  disk-full, or power-loss validation. Injected failures are covered at the
  adapter level only (see [Failure handling](#failure-handling)).
- The desktop composition root is not wired to this adapter yet; the media backend
  decision and later composition phase must provide a compatible pipeline.
