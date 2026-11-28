# Framelet implementation roadmap

**Status:** Binding delivery plan  
**Last updated:** 27 November 2026 (re-sequenced; see [ADR-0018](adr/ADR-0018-risk-first-resequencing-and-two-stage-media-decision.md))  
**Architecture authority:** [arc42](arc42.md), [ADR index](adr/README.md)  
**Verification authority:** [Verification framework](../verification/README.md)  
**Mandatory implementation style:** Domain-Driven Design (DDD) inside the application core, hexagonal architecture (ports and adapters), safe Rust, local-first operation.

This roadmap defines the implementation order, dependencies, deliverables, exit criteria, and — for every phase — **how it is proven that the phase is correct and complete**. A phase is not complete merely because code compiles or a checkbox is ticked: every exit criterion must point to verifiable evidence (a named automated test, a CI job, or an evidence record). Re-plan through an ADR when evidence requires a structural change.

## Why this plan was re-sequenced

The previous plan (Phases 0–8) made every capture phase wait for an open-ended media-backend evaluation whose own exit gates (real-capture drift, clean-machine packaging, plugin licensing) could only be satisfied after the capture phases. That was circular. The largest technical risks — Windows capture without first-party `unsafe`, webcam support, and the redistributability of H.264/AAC — had no prototype at all, while the application could not record anything after 26 PRs. The new order:

1. closes architecture debt in the core before real adapters build on it,
2. retires the largest unknowns early with time-boxed spikes that end in Go/No-Go,
3. delivers a real, recordable walking skeleton as soon as possible,
4. replaces "defer until everything is proven" with a reversible **provisional** backend decision and a later **production** approval (ADR-0018).

## Planning and tracking policy

The project plan, phase breakdown, progress states, decisions, and open work are maintained as Markdown files in this Git repository (this roadmap, arc42, the specification, ADRs, and the [verification records](../verification/README.md)). **Do not create or maintain GitHub Issues for project planning, tasks, bugs, phase tracking, or backlog management.** Update the relevant Markdown document in the same PR as the implementation change. Pull requests remain the mechanism for proposing, reviewing, and delivering code.

## How correctness and completeness are proven

The detailed rules are in the [verification framework](../verification/README.md). In short:

| Question | Proof mechanism |
|---|---|
| **Complete?** Is every requirement of the phase implemented? | Every requirement has a stable ID in the [traceability matrix](../verification/traceability.md) and is assigned to exactly one phase. A phase is complete only when every row assigned to it is in state *verified*, i.e. links to an existing automated test and, where the row demands it, an evidence record. From Phase A on, `scripts/check-traceability.py` enforces in CI that every linked test exists. |
| **Correct?** Does it behave as specified? | Each row names its verification method: unit/aggregate test, exhaustive or property-based test, shared port contract test suite, fault-injection test, output-file inspection, real-device evidence record, or measurement. The method must be able to fail: tests assert outcomes and call order, not just "no panic". |
| **Correct on the real platform?** | Evidence records (`docs/verification/evidence/`) using the template, with commit SHA, OS/compositor, hardware, device models, procedure, observations, and raw artifacts (for example `ffprobe` output). CI never substitutes for a real-device record. |
| **Strong enough tests?** | Mutation testing on `framelet-core` (introduced in Phase A after tool review) with every surviving mutant killed or justified; fakes must pass the same contract suite as real adapters. |
| **Architecture still intact?** | `scripts/check-architecture.py` (dependency allowlist, forbidden references, unsafe declarations) and reviewer sign-off against AGENTS.md. |

Track four separate evidence states per requirement: **implemented**, **automated-tested**, **UI-integrated**, **real-device-validated**. Never conflate them.

**Phase closure:** a phase is closed by a dedicated PR (or the last slice PR) that (1) sets every criterion's evidence link, (2) updates the traceability matrix, (3) updates this roadmap's status table, and (4) carries a reviewer sign-off. A criterion without a link stays unchecked.

## Delivery principle: vertical, UI-testable increments

From Phase C on, user-visible capabilities MUST be delivered as vertical slices. Each slice connects the real capability through its adapter, core use case, desktop composition root, and Slint UI in the same PR (or a clearly linked, immediately following PR when a dependency gate makes that impossible).

### Slice Definition of Done (applies to every slice from Phase C on)

This replaces the former cross-cutting "Phase 7". Each slice PR must show:

- [ ] The capability is reachable in the running desktop app with real state and typed error feedback; no mock success state.
- [ ] Not-yet-implemented controls are visibly disabled with an explanation; no dead buttons or fake selectors.
- [ ] The composition root is the only place constructing the new adapter; no UI-to-adapter shortcut, no service locator (verified by `check-architecture.py` and review).
- [ ] No capture, encoding, enumeration, or filesystem work on the UI thread (verified by code review of every new UI callback and a UI responsiveness check in the manual procedure).
- [ ] The adapter passes the shared port contract suite (see Phase A).
- [ ] Traceability rows for the slice are updated; manual UI test procedure is written down; an evidence record exists for each OS/device combination claimed.
- [ ] Previously working UI flows still pass their procedures (regression).
- [ ] arc42, this roadmap, and affected ADRs are updated in the same PR.

## Delivery rules applying to every phase

1. Keep the domain model, ubiquitous language, invariants, and application use cases in the core. Define ports in application-owned code.
2. Apply DDD pragmatically: identify entities/value objects and aggregate boundaries from real invariants; keep domain logic framework-free.
3. Implement platform, UI, media, and storage integrations as adapters. The desktop executable is the only composition root.
4. Core must not depend on Slint, CPAL, PipeWire, OS APIs, codecs, muxers, or filesystem implementation types. No adapter-to-adapter coupling.
5. No first-party `unsafe`, raw pointers, custom FFI, unbounded media queues, or blocking UI/audio callbacks.
6. Translate external types/errors at adapter boundaries. Use typed errors and owned data across asynchronous boundaries.
7. Test domain invariants directly; test use cases with fakes; test adapters against the shared contract suite; test platform behavior on real devices.
8. Update the relevant arc42 sections and add an ADR for significant decisions in the same PR.
9. Do not claim a platform, mode, or backend as supported until its real-device and packaging gates pass.

## Phase overview

| Phase | Scope | Status | Depends on |
|---|---|---|---|
| — | Foundation (former Phases 0, 1, Phase 2 PoC work, Phase 3 adapter slice) | Delivered; formal verification folded into Phase A | — |
| A | Core consolidation: port-based orchestration, timeline model, data-plane and storage/media hand-off contracts, contract test suites, traceability tooling | In progress — A-1…A-8, A-10 done (ADR-0020/0021/0022 accepted 2026-11-28); A-9 awaits a mutation run on a committed SHA | — |
| B | Risk spikes with Go/No-Go: Windows capture (safe), webcam, encoder/licence matrix | Planned | — (parallel to A) |
| C | Walking skeleton: microphone → WAV (interim) → storage → UI; settings and output directory | Planned | A |
| D | Provisional media backend; microphone → M4A/AAC; packaging pipeline starts | Planned | B, C |
| E | Screen capture slices: E1 Wayland, E2 Windows | Planned | D (E2 also B1 = Go) |
| F | Webcam-only slice with preview | Planned | D, B2 = Go |
| G | Screen + webcam composition (picture-in-picture) | Planned | E, F |
| H | Hardening, measurement, production backend approval, release | Planned | C–G |

Cross-cutting from Phase D on: the packaging pipeline (CI-built installable artifacts for Windows and Linux) runs on every PR that changes native dependencies, so packaging risk is visible long before Phase H.

Parallel work is allowed only where the port contracts it depends on are stable (Phase A deliverables A2–A4 for any adapter work).

## Foundation — delivered before re-sequencing

What exists and stays valid:

- Backend-neutral owned audio/video contracts, bounded worker-level audio FIFO (former Phase 0, PR #12).
- Recording aggregate, inbound use-case port, granular outbound ports (former Phase 1, PR #13).
- Isolated GStreamer PoC with bounded producer queue, EOS, filesink-error and timeout tests (PRs #14, #15, #18; ADR-0013, ADR-0015).
- Isolated Wayland portal/PipeWire PoC and CPAL microphone PoC including WAV writer (PRs #6–#11).
- Transactional filesystem storage adapter with fault-injection seam; desktop recovery scan (PRs #16, #17, #22, #26; ADR-0014).

The former Phase 0 and Phase 1 exit checklists were never linked to evidence (Phase 0 was marked complete with no criterion checked). Their verification is part of Phase A criterion A-V1 rather than being re-ticked retroactively. Open items from former Phases 2 and 3 are re-assigned below:

| Former open item | New home |
|---|---|
| Independent-player playback, target-format validation | D (provisional, Linux + Windows smoke), H (full matrix) |
| Encoder-specific errors, interruption, incomplete-output cleanup | D |
| Sustained overload, control-path independence, measurements | D (first measurements), H (targets) |
| Native plugin inventory and redistribution licences | B3 (matrix), D (provisional profile), H (production approval) |
| Native output-directory selection | C2 |
| Desktop/media integration and closed-writer hand-off | A4 (contract), D (implementation) |
| Real-filesystem fault runs, durability and power-loss | H |

## Phase A — Core consolidation

**Goal:** the core contracts that every adapter will build on are complete, typed, and testable, so no adapter is written against a contract that will change.

Deliverables:
- **A1 — Port-based orchestration (delivered in this PR, ADR-0017).** `RecordingApplication` coordinates the aggregate exclusively through the granular ports (`RecordingStoragePort`, `MediaPipelinePort`, screen/camera/microphone capture ports, `RecordingEventPublisher`). The former `RecordingResources` facade and its `String` errors are removed. Failures are typed (`CapabilityFailure { capability, error: PortError }`), rollback failures are reported rather than swallowed, capture ports are optional so a build can offer only implemented capabilities, and source selection must match the configuration exactly (domain invariant).
- **A2 — Recording timeline model.** Domain value objects for the common origin T0, pause segments, and the mapping from source monotonic timestamps to presentation timestamps (paused duration removed), using the `MonotonicClock` port. Discontinuity and regression detection produce typed domain facts.
- **A3 — Data-plane contract.** How owned `VideoFrame`/`AudioBlock` values travel from capture adapters to the media pipeline without adapter-to-adapter coupling (for example core-owned bounded sink handles passed at start), including per-stream overload policy (video: counted drop; audio: explicit discontinuity) and the `SourceLost` event path. Recorded as an ADR. *Implemented:* `crates/core/src/data_plane.rs`, `process_source_losses`; [ADR-0020](adr/ADR-0020-bounded-data-plane-and-source-loss.md) (Proposed).
- **A4 — Storage/media hand-off contract.** How the media pipeline receives its output target from the storage reservation and signals a closed writer before `publish`, without the composition root reaching into concrete adapter methods. Recorded as an ADR; `MediaPipelinePort::prepare` and `RecordingStoragePort` are adjusted accordingly. *Implemented:* `StorageReservation`/`OutputTarget`/`ClosedOutput` in `crates/core/src/ports.rs`; [ADR-0021](adr/ADR-0021-storage-media-hand-off-and-contract-suites.md) (Proposed).
- **A5 — Shared port contract suites.** Reusable, adapter-agnostic test functions per port (lifecycle order, idempotent stop/abort, error mapping, no publication before close) that both the fakes and every real adapter must pass. The storage adapter is the first real adapter to run them. *Delivered:* `crates/core/src/contract/` (`contract-tests` feature, ADR-0021); `crates/storage/tests/contract.rs`.
- **A6 — Remaining ports wired.** `RecordingSessionRepository`, `RecordingSettingsPort`, `RecordingMetricsPort`, and `MonotonicClock` are either used by a use case or explicitly documented as not yet consumed, with the consuming phase named.
- **A7 — Verification tooling.** `scripts/check-traceability.py` in CI; requirement IDs added to the traceability matrix for all of SPECIFICATION.md; mutation-testing tool reviewed per the dependency policy and run on `framelet-core`.

Exit criteria and proof:

| ID | Criterion | Proof (must exist and pass) |
|---|---|---|
| A-1 | No use case depends on a facade or `String` error; every outbound interaction goes through a granular port | [x] `RecordingResources` removed; `crates/core/src/application.rs`; `check-architecture.py`; review |
| A-2 | Orchestration order is specified and tested for every lifecycle path | [x] `application_tests.rs`: `full_lifecycle_orchestrates_ports_in_order_and_publishes_only_after_finalization`, `each_mode_starts_exactly_its_required_sources` |
| A-3 | Every failure path ends in a defined state, releases resources, never exposes an artifact, and reports secondary failures | [x] `publication_failure_…`, `drain_failure_…`, `capture_stop_failure_…`, `capture_start_failure_…`, `rollback_failures_are_reported_not_swallowed`, `pipeline_prepare_failure_…`, `cancel_with_cleanup_failure_…` |
| A-4 | Illegal commands never reach a port | [x] `illegal_commands_never_reach_a_port` |
| A-5 | Lifecycle transition table is exhaustively enforced | [x] `domain::tests::transition_table_is_exhaustively_enforced` (all 81 state pairs) |
| A-6 | Timeline mapping is correct for arbitrary pause/resume sequences, never overflows, and detects regressions | [x] `framelet_core::timeline::tests::exhaustive_single_stream_sequences_up_to_six_ops_match_oracle`, `framelet_core::timeline::tests::exhaustive_multi_stream_sequences_up_to_five_ops_match_oracle`, `framelet_core::timeline::tests::seeded_random_long_sequences_match_oracle` (u128 oracle over grids `0..=6` and `u64::MAX-6..=u64::MAX`), `framelet_core::timeline::tests::maps_at_u64_max_without_overflow`, `framelet_core::timeline::tests::regression_and_duplicate_are_detected_without_advancing_cursor` |
| A-7 | Data-plane and hand-off contracts are complete | [x] ADR-0020 and ADR-0021 accepted by the maintainer on 2026-11-28 ([ADR-0020](adr/ADR-0020-bounded-data-plane-and-source-loss.md), [ADR-0021](adr/ADR-0021-storage-media-hand-off-and-contract-suites.md)); contract suites: `framelet_core::contract::tests::fake_media_video_overload_counts_drops`, `fake_media_audio_overload_yields_discontinuity`, `fake_media_audio_trailing_loss_is_reported_after_stream_end`, `fake_media_queues_never_exceed_capacity`, `fake_screen_capture_passes_capture_contract` (source loss through the sink), `fake_storage_rejects_foreign_closed_output`; `crates/storage/tests/contract.rs` (`filesystem_storage_rejects_foreign_closed_output`, `filesystem_storage_passes_the_whole_storage_suite`); application tests `essential_source_loss_stops_without_publishing_and_keeps_candidate`, `optional_microphone_loss_continues_and_is_recorded_on_the_session`, `foreign_closed_output_is_never_published` |
| A-8 | Fakes and real adapters agree | [x] `crates/storage/tests/contract.rs` runs every `framelet_core::contract::storage` suite against `FileSystemRecordingStorage` (`framelet_storage::contract::filesystem_storage_passes_the_whole_storage_suite` and one test per suite); the fakes pass the storage, media, and capture suites in `framelet_core::contract::tests`; both run under `cargo test --workspace --locked` in the `linux` and `windows` CI jobs |
| A-9 | Tests are strong enough | [ ] Mutation run on `framelet-core`: zero surviving mutants, or each survivor justified in the evidence record. Note: implementation and evidence done on uncommitted tree based on 6205b0c ([record](../verification/evidence/2026-11-27-linux-container-mutation-core.md)); ADR-0022 was accepted by the maintainer on 2026-11-28; stays unchecked until a mutation run on a committed SHA (CI full run on main) is recorded. |
| A-10 | Completeness is machine-checked | [x] `scripts/check-traceability.py` (inline `[REQ-…]` tags in SPECIFICATION.md, every tag has a matrix row with a valid phase and state), self-tested by `scripts/test_check_traceability.py`; both run in the `linux` and `windows` jobs of `.github/workflows/ci.yml` |
| A-V1 | Foundation verified | [ ] Link to a green `main` CI run (Linux + Windows, audits) recorded in `docs/verification/evidence/` |

## Phase B — Risk spikes (Go/No-Go)

**Goal:** retire the three unknowns that can invalidate the product scope before effort is invested in slices that depend on them. Each spike is time-boxed (target: one to two weeks of effort), lives under `poc/`, and ends with an ADR stating **Go**, **No-Go**, or **Go with scope change**.

- **B1 — Windows screen capture without first-party unsafe.** Most `windows`-crate COM/WinRT calls are `unsafe fn`, so Windows Graphics Capture must come through a reviewed wrapper that exposes a safe API. Prototype display and window capture, user-mediated picker, frame delivery into owned `VideoFrame`s, stop/release. If no safe wrapper is viable, the ADR must state the consequence for Windows support (AGENTS.md: stop and explain, never work around).
- **B2 — Webcam.** Enumerate cameras, negotiate formats, deliver owned frames, and handle unplug on Windows and Linux with nokhwa and at least one alternative (for example a PipeWire camera portal on Linux).
- **B3 — Encoder and licence matrix.** For MP4/H.264/AAC and at least one royalty-free alternative (for example WebM/VP9/Opus), list per OS which encoders are available (OS-provided such as Media Foundation via the media runtime, OpenH264, x264, FDK/AAC alternatives), their licences, patent and redistribution implications, and packaging footprint. The result may change the default output format in SPECIFICATION.md §4.5/§6.3 — that is the point of doing it early.

Exit criteria and proof:

| ID | Criterion | Proof |
|---|---|---|
| B-1 | Each spike has a decision | [ ] One ADR per spike with Go/No-Go and the evidence it relied on |
| B-2 | Decisions rest on real execution, not documentation | [ ] Evidence record per spike and OS: commit SHA, OS build, hardware, device model, commands, observed output (frame count, formats, errors) |
| B-3 | Safety constraint holds | [ ] Spike crates `forbid(unsafe_code)`; dependency review (licence, advisories, unsafe/FFI surface) documented per AGENTS.md dependency policy |
| B-4 | Failure modes observed, not assumed | [ ] Each spike's evidence includes cancel/deny and disconnect behaviour |
| B-5 | Consequences propagated | [ ] If any spike is No-Go or changes scope, SPECIFICATION.md, this roadmap, and the traceability matrix are updated in the same PR |

## Phase C — Walking skeleton

**Goal:** a user can record something real from the desktop app end to end, so the lifecycle, finalization, storage, recovery, and non-blocking UI are proven with real data before the media backend is chosen.

Deliverables:
- **C1 — Microphone → WAV.** Promote the CPAL PoC into a production microphone adapter after dependency review (explicit device selection, format negotiation, callback → SPSC ring → worker → bounded queue). An interim WAV media adapter implements `MediaPipelinePort` using the A3/A4 contracts. The UI labels WAV explicitly as an interim format. Start, pause, resume, stop, and cancel work from the UI; the completed path is shown only after publication.
- **C2 — Settings and output directory.** TOML settings adapter behind `RecordingSettingsPort` (validated, malformed files handled safely); output directory selection through a reviewed native dialog adapter; recovery scan uses the selected directory.

Exit criteria and proof:

| ID | Criterion | Proof |
|---|---|---|
| C-1 | Slice Definition of Done met | [ ] Checklist above, linked in the slice PR |
| C-2 | Output is a valid, complete file whose duration matches the recording minus pauses | [ ] Automated test with a synthetic microphone fake: WAV header/sample count matches the timeline model (A2) within one block; real recording inspected with `ffprobe` or `soxi` in the evidence record |
| C-3 | No silent audio loss | [ ] Test forcing ring/queue overflow yields a typed discontinuity event and a degraded/failed outcome, never Completed-without-warning |
| C-4 | Callback is real-time safe | [ ] Code review checklist for the callback (no allocation, lock, I/O, logging); test counting allocations in the callback path, or documented static argument if not measurable |
| C-5 | Device loss is handled | [ ] Evidence record: unplug during recording on Linux and Windows → typed `SourceLost`, safe stop, recoverable output |
| C-6 | Real-device validation | [ ] Evidence records for the reference matrix entries claimed (at least one Linux Wayland and one Windows machine) using the C1 manual procedure, 10-minute standard run |
| C-7 | Settings are robust | [ ] Tests for missing, malformed, out-of-range, and unwritable settings files |

Note (C-7): no settings adapter exists yet. C2 names a TOML adapter, but the owning crate, the serialization dependency (ADR + dependency review) and the on-disk schema/default and out-of-range policy are not decided, so C-7 cannot be tested. Phase C as a whole is not complete.

## Phase D — Provisional media backend

**Goal:** select a media backend provisionally (ADR-0018 stage 1) and ship microphone recording in the target audio format. The decision stays reversible because only one adapter crate depends on the backend.

Deliverables:
- Provisional backend ADR based on the B3 matrix and existing PoC evidence; the backend is used only in a `media-*` adapter crate.
- Microphone → M4A/AAC (or the format chosen in B3) replacing the interim WAV path; WAV may remain as a diagnostic option only if documented.
- Encoder-specific error injection, process interruption, and incomplete-output cleanup tests.
- First measurements (CPU, RSS, start latency, stop-to-finalize) on the reference machines.
- Packaging pipeline: CI builds installable Windows and Linux artifacts including the backend's native runtime and an explicit plugin allowlist; clean-machine smoke install.

Exit criteria and proof:

| ID | Criterion | Proof |
|---|---|---|
| D-1 | Provisional selection is justified and reversible | [ ] ADR with exact versions, plugin set, licences; `check-architecture.py` proves only the adapter crate depends on the backend |
| D-2 | Output plays and is well-formed | [ ] Automated `ffprobe`-style inspection in CI of a synthetic recording (container, codec, duration, stream count); evidence records of playback in two independent players per OS |
| D-3 | Failures never produce false success | [ ] Integration tests: encoder error, sink error, EOS timeout, killed process → no Completed state, candidate kept for recovery |
| D-4 | Backpressure is bounded under sustained load | [ ] Stress test with a slow consumer: queue high-water ≤ capacity, video drops counted, audio discontinuity explicit, control commands (stop) still processed within a stated bound |
| D-5 | Packaging works on clean machines | [ ] Evidence record of installing and recording on a clean Windows VM and a clean Linux VM without manually installed runtimes |
| D-6 | Measurements recorded | [ ] Evidence record with hardware/software context; deviations from SPECIFICATION.md §8 targets listed, not hidden |

## Phase E — Screen capture slices

**Goal:** OS-mediated screen/window recording from the desktop app, Wayland first (E1), then Windows (E2, only if B1 = Go). Microphone paths must keep working.

Deliverables:
- E1: production Wayland adapter (XDG Desktop Portal selection, PipeWire stream), promoted from the PoC after dependency review.
- E2: production Windows Graphics Capture adapter through the wrapper approved in B1.
- Source selection, capability reporting, stride/format validation, cancellation vs. denial, disconnect handling, bounded frame queues with counted drops.
- X11 remains out of scope until a separate decision.

Exit criteria and proof:

| ID | Criterion | Proof |
|---|---|---|
| E-1 | Slice Definition of Done met | [ ] Checklist linked in each slice PR |
| E-2 | Frame validation is total | [ ] Tests for zero/oversized dimensions, short buffers, stride mismatch, unsupported formats, arithmetic overflow; fuzz or property tests on the conversion function |
| E-3 | Cancel and deny are distinct | [ ] Adapter contract test with scripted portal/OS responses; evidence record of both on real desktops |
| E-4 | A/V sync with microphone | [ ] Evidence record: clap/flash test or timecode overlay, measured offset < 100 ms at start and end of a 10-minute recording |
| E-5 | Real platforms | [ ] E1: evidence records for GNOME and KDE Wayland; E2: Windows 10 and 11; display and window capture each |
| E-6 | No permission bypass, no first-party unsafe | [ ] Review; `check-architecture.py`; dependency review record |

Note (E-2, partial): `VideoFrame::new` has boundary, overflow, exhaustive small-layout and seeded property tests against an independent `u128` reference predicate, and enforces the 8K limit decided on 2026-11-28 (each side ≤ 7680 and width × height ≤ 7680 × 4320, so portrait 8K is accepted; `VideoFrameTooLarge`; REQ-CAP-05/06 in the traceability matrix) before any stride or length arithmetic. E-2 stays open because the criterion names "the conversion function": no production native-to-`VideoFrame` conversion function exists yet, so "rejected before allocation" and fuzz/property tests on that function cannot be shown until a Phase E adapter calls `validate_frame_dimensions` before allocating. Phase E as a whole is not complete.

## Phase F — Webcam slice

**Goal:** webcam-only recording with live preview (SPECIFICATION.md §4.2 step 5) from the desktop app. Independent of screen capture.

Exit criteria and proof:

| ID | Criterion | Proof |
|---|---|---|
| F-1 | Slice Definition of Done met | [ ] Checklist linked |
| F-2 | Format negotiation is explicit | [ ] Tests: device offers no supported format → typed `UnsupportedFormat`; the UI never claims a setting the device cannot deliver |
| F-3 | Preview does not block UI or recording | [ ] Preview frames go through a bounded, drop-oldest preview path; UI responsiveness check in the manual procedure |
| F-4 | Hot-unplug | [ ] Evidence record per OS: unplug during preview and during recording |
| F-5 | Real platforms | [ ] Evidence records for one UVC USB and one built-in camera per claimed OS |

## Phase G — Screen + webcam composition

**Goal:** picture-in-picture recording per SPECIFICATION.md §4.4.

Exit criteria and proof:

| ID | Criterion | Proof |
|---|---|---|
| G-1 | Layout is deterministic | [ ] Golden-frame tests: for each corner × size, synthetic screen and camera frames produce a pixel-exact (or tolerance-bounded) expected composition; aspect ratio preserved; mirroring affects only the preview |
| G-2 | Streams stay synchronized | [ ] Synthetic timeline test with differing frame rates; evidence record with a 30-minute run measuring drift |
| G-3 | Resource use bounded | [ ] Long-run evidence (60 minutes): RSS slope ≈ 0, queue high-water ≤ capacity |
| G-4 | All four modes coexist | [ ] Regression procedures for screen, microphone, webcam, and screen + webcam all pass in the same build |

## Phase H — Hardening and release

**Goal:** the MVP Definition of Done (SPECIFICATION.md §12) holds on every advertised platform, and the media backend is approved for production (ADR-0018 stage 2).

Deliverables:
- Full scenario matrix: four modes × optional microphone × pause/resume/stop/cancel × unplug, disk full, permission denial, crash recovery.
- Long-duration and stress runs; final measurements against SPECIFICATION.md §8.
- Real-filesystem fault and power-loss runs on NTFS, ext4, btrfs.
- Offline/network observation and privacy review; X11 tested or explicitly marked unsupported.
- Production backend approval ADR; reproducible, verified packages; release notes listing unsupported capabilities.

Exit criteria and proof:

| ID | Criterion | Proof |
|---|---|---|
| H-1 | Every advertised OS/mode has real-device evidence | [ ] Traceability matrix: no row assigned to the MVP is below *real-device-validated* where the row requires it |
| H-2 | No false success | [ ] Fault matrix results in evidence records; every injected failure ends Failed or degraded with a visible message |
| H-3 | Offline and private | [ ] Evidence record of a full recording with network disabled and a network trace showing no connection attempts; log review shows no captured content |
| H-4 | Targets measured | [ ] Measurement evidence with hardware/software context; misses documented |
| H-5 | Supply chain reviewed | [ ] `cargo audit`, `cargo deny`, `cargo vet` pass; native runtime/plugin inventory with licences published |
| H-6 | Docs match shipped behaviour | [ ] arc42, ADRs, SPECIFICATION.md and README reviewed against the release build; unsupported list present |

## Change control

A phase may be split, reordered, or deferred only with documented evidence. If the change alters dependency direction, lifecycle ownership, trust boundaries, media time model, backend selection, or platform support, create a new ADR and update arc42 before or alongside implementation. The hexagonal dependency rule, the 12-section arc42 requirement, and the proof requirement for exit criteria cannot be waived by phase planning.
