# Transactional filesystem storage adapter

This crate implements the core-owned `RecordingStoragePort`. It is an outbound
adapter, not part of the domain model, and does not choose a media runtime, codec,
or container.

## Lifecycle

1. `begin` reserves a unique temporary `.part` file using exclusive creation.
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

## Limitations

- The temporary path is a hand-off seam for the media adapter; `publish` must not
  be called until all media writers are closed.
- Directory fsync and power-loss durability semantics are platform/filesystem
  dependent and are not yet claimed.
- Hosted CI does not replace real-device, filesystem fault-injection, or power-loss
  validation.
- The desktop composition root is not wired to this adapter yet; the media backend
  decision and later composition phase must provide a compatible pipeline.
