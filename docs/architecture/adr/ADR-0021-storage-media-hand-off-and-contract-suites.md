# ADR-0021: Storage-to-media hand-off through the core, and shared port contract suites

- **Date:** 27 November 2026
- **Status:** Proposed — implemented for review; a human maintainer must accept it before roadmap criterion A-7 can be ticked
- **Amends (on acceptance):** ADR-0012 decision 4 (storage publishes after the media writer closed — now enforced by contract plus target check); ADR-0014 "the media adapter obtains the temporary path" and its `abort` semantics; ADR-0017 decision 5 (prepare passes the output target; stop publishes a `ClosedOutput`; cancel/rollback use `media.abort`). The accepted text of those ADRs is not edited.
- **Related:** ADR-0012, ADR-0014, ADR-0017, ADR-0020; roadmap Phase A deliverables A4 and A5

## Context

The filesystem storage adapter reserved a temporary candidate, but the only way for a media adapter to learn where to write was the concrete method `FileSystemRecordingStorage::temporary_path`, which the composition root would have had to call and pass on — the composition root reaching into an adapter and coupling two adapters through it. "Publish only after the media writer has closed" (SPECIFICATION.md §7, REQ-STOR-02) was a comment: `RecordingStoragePort::publish(&session)` could be called at any time, and storage and media both used `MediaSessionId`, so the two identities could be mixed up. Storage `abort` failed for a session that was already aborted or published, so cleanup was not idempotent. Finally, fakes lived inside `application_tests.rs`; no test showed that the fakes behave like the real adapter (roadmap A5, A-8).

## Decision

### Hand-off

1. `RecordingStoragePort::begin` returns `StorageReservation { session: StorageSessionId, target: OutputTarget }`. `StorageSessionId` is a distinct type from `MediaSessionId`.
2. `OutputTarget` wraps a `std::path::PathBuf` chosen by the storage adapter. Core forwards it unchanged to `MediaPipelinePort::prepare(id, &configuration, &target)` and only ever compares targets; it never opens or derives paths. A `PathBuf` (not `String`) is used because non-UTF-8 directories are legal on Linux and Windows and a lossy conversion would make the media writer open a different file.
3. `MediaPipelinePort::close(&session) -> Result<ClosedOutput, PortError>`. The session is released whether or not finalization succeeds. `ClosedOutput` names the target whose writer has drained and closed; it is not `Clone`, so one close permits one publication attempt.
4. `RecordingStoragePort::publish(&StorageSessionId, ClosedOutput) -> Result<RecordingArtifact, PortError>`. The closed output is required by value and only `close` produces it in the application flow. Because `ClosedOutput::new` is public, publish-after-close is enforced *by contract plus target check*, not by the type alone: storage rejects with `PortError::InvalidInput`, before any filesystem call and without publishing, a `ClosedOutput` whose target is not the one reserved for that session (foreign session or never-issued target), and an inactive session. The application also checks `closed.target() == reservation.target` before publishing and fails the session otherwise.
5. `MediaPipelinePort::abort(&session)` releases a pipeline without producing a closed output (cancel, start rollback, prepare rollback). Cleanup of the partial candidate stays with `storage.abort`.
6. **Idempotent release.** `storage.abort`, `media.abort`, and capture `stop` succeed without effect for a session that is not active (already released, published, or never issued). `storage.abort` after `publish` never touches the published recording. A first `abort` whose filesystem removal fails still reports the error and keeps the session for retry (ADR-0014 failure table unchanged).

`ClosedOutput::new` is public because a media adapter in another crate must construct it; the guarantee is about the application flow and the storage check, not about making forgery impossible. A forged `ClosedOutput` for the right target is equivalent to a media adapter lying about having closed its writer, which the media contract suites and output inspection (Phase D) address.

### Shared port contract suites

7. `framelet-core` has a `contract-tests` feature (no dependencies) and a module `framelet_core::contract` compiled under `#[cfg(any(test, feature = "contract-tests"))]`:
   - `contract::fakes` — the in-memory fakes for every outbound port (`FakeStorage`, `FakeMedia`, `FakeCapture<VideoSink|AudioSink>`, `FakeEvents`, `Log`, `Faults`), used by the core's application tests;
   - `contract::storage`, `contract::media`, `contract::capture` — suites as `pub fn`s taking a factory for a fresh *subject* (the adapter plus the test seams a suite needs, for example writing media to a target in place of a media writer).
8. Storage suites: reservations are exclusive; begin → write → close → publish exposes exactly the written bytes once; abort is idempotent and spares published output; foreign or never-issued closed outputs are rejected and leave both reservations publishable; unknown sessions are rejected (publish) or ignored (abort). Media and capture suites are described in ADR-0020.
9. Fakes pass the same suites as real adapters (`framelet_core::contract::tests`). An adapter crate runs them by enabling the feature **only** in `[dev-dependencies]`; `scripts/check-architecture.py` fails if any first-party manifest enables `contract-tests` in `[dependencies]` or `[build-dependencies]`, so fakes never reach a production build. `framelet-storage` does this in `crates/storage/tests/contract.rs`, running every storage suite against `FileSystemRecordingStorage` in a temporary directory.

## Alternatives

- **Composition root passes `temporary_path` to the media adapter:** rejected; the root would depend on adapter internals and couple two adapters.
- **Storage hands out a writable file handle instead of a location:** rejected for now; a GStreamer/FFmpeg-style muxer opens its own sink by path, and a `std::fs::File` crossing the port would still need the path for diagnostics. Revisit with the Phase D backend.
- **`OutputTarget` as `String`:** rejected; see decision 2.
- **Sealed `ClosedOutput` constructible only through a core-issued token:** rejected as over-engineering at this stage; the storage target check plus the application check already reject every mismatch the application can produce.
- **Abort of an unknown session as an error:** rejected; abort's postcondition (no candidate owned by this session remains) holds, and an error would make retries and double cleanup paths fail spuriously. Typed errors remain for `publish`.
- **Suites in a separate `framelet-contract-tests` crate:** rejected; the suites need only the core's public API and its fakes, and a feature-gated module avoids another workspace member and keeps the fakes next to the contracts they model.

## Consequences

- The media adapter learns its output target only through the core; the desktop composition root constructs adapters and never reaches into them.
- `FileSystemRecordingStorage::temporary_path` is removed from the public API; the target is part of the reservation.
- Every port change is now checked against two implementations (fake and real) by the same assertions, so fakes cannot drift from adapters silently.
- `cargo test --workspace` builds `framelet-core` with `contract-tests` enabled for test targets; release builds do not include it.
- The final file name (`.framelet`) is still chosen by storage without knowing the container; container-specific naming follows the Phase D backend decision.

## Validation

- `crates/storage/tests/contract.rs`: `filesystem_storage_reservations_are_exclusive`, `filesystem_storage_publishes_closed_output_once`, `filesystem_storage_abort_is_idempotent_and_spares_published_output`, `filesystem_storage_rejects_foreign_closed_output`, `filesystem_storage_rejects_unknown_session`, `filesystem_storage_passes_the_whole_storage_suite`.
- The same suites against the fake: `framelet_core::contract::tests::fake_storage_*`.
- Fault tests: `repeated_abort_and_abort_after_publish_touch_no_file`, `foreign_closed_output_is_rejected_before_any_filesystem_call`; existing ADR-0014 fault table tests adapted.
- Application tests: `foreign_closed_output_is_never_published`, `full_lifecycle_orchestrates_ports_in_order_and_publishes_only_after_finalization`, `cancel_releases_prepared_resources_and_blocks_start`.
- Manual mutations during review (accepting a foreign closed output, non-idempotent abort, uncounted video drop) each made the corresponding suite fail.
- `scripts/check-architecture.py` checks the feature is confined to `[dev-dependencies]`.
