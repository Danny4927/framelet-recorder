# TOML settings adapter

This crate implements the core-owned `RecordingSettingsPort` (roadmap C2,
[ADR-0023](../../docs/architecture/adr/ADR-0023-toml-settings-persistence.md)).
It is an outbound adapter: the settings types, their invariants, and their
defaults live in `framelet_core::settings`; this crate owns the file format,
the platform location, and the atomic write. It is the only first-party crate
allowed to depend on `serde` and `toml` (`scripts/check-architecture.py`).

## Location

| Platform | File |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/framelet/settings.toml`, else `$HOME/.config/framelet/settings.toml` |
| Windows | `%APPDATA%\Framelet\settings.toml` |

Only set, non-empty, absolute environment values are used. Otherwise
`TomlSettingsFile::from_environment` returns `LocationUnavailable`.

## Schema version 1

```toml
schema_version = 1                       # required; any other value is rejected
microphone_enabled_by_default = false    # optional, default false
output_directory = "/home/me/Videos/Rec" # optional; absent = platform default
```

Unknown keys and wrong types are rejected. A new key requires a new schema
version.

## Load

`load` never fails, never panics, and never writes.

| Situation | Result |
| --- | --- |
| No file | defaults, `DefaultsNotStored` (not an error) |
| Valid file | stored settings, `Stored` |
| Larger than 64 KiB | defaults + `TooLarge` (not parsed) |
| Not UTF-8 / invalid TOML | defaults + `Malformed { line }` |
| No / unknown `schema_version` | defaults + `MissingSchemaVersion` / `UnsupportedSchemaVersion` |
| Unknown key / wrong type | defaults + `SchemaMismatch { line }` |
| Invalid `output_directory` | defaults + `InvalidOutputDirectory(violation)` (rejected, not clamped) |
| Unreadable | defaults + `ReadFailed(..)` |
| Directory, FIFO, socket, or device at the path | defaults + `ReadFailed(NotAFile)`, checked before opening so a FIFO cannot block |

No field of a rejected file is applied, and the file is left unchanged.

## Save

1. Encode in memory.
2. Refuse a read-only `settings.toml` or a directory at its path.
3. Create the directory if needed (Unix `0700`).
4. Create `.settings.toml.<pid>.<n>.tmp` exclusively (Unix `0600`).
5. Write, `sync_all`, close, rename over `settings.toml`.

Any failure returns `WriteFailed`, removes the temporary file, and leaves the
previous `settings.toml` unchanged. `settings.toml` is always the complete old
or the complete new file, never partial; because the directory is not fsynced,
a completed save can revert to the old file after a power loss. All write calls go through the private
`FileOperations` seam (`src/file_ops.rs`) so tests inject a failure at every
step (`src/tests.rs`).

## Limitations

- Not yet composed in `apps/desktop` (C2 slice).
- User comments in the file are dropped on save.
- The directory is not fsynced; power-loss durability is not claimed.
- Concurrent saves from two processes: each writes a complete file, last wins.
- A symlinked `settings.toml` is read through, but a save replaces the link
  (as it replaces a FIFO or socket at the path).
- Check-then-act races are not closed: a FIFO swapped in between the type
  check and the open can still block a load, and a change between the
  read-only check and the rename is not detected (ADR-0023).
- The Unix permission tests skip (with a message) when run as root.
