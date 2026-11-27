# Framelet implementation roadmap

**Status:** Binding delivery plan  
**Last updated:** 17 November 2026  
**Architecture authority:** [arc42](arc42.md), [ADR index](adr/README.md)  
**Mandatory implementation style:** Domain-Driven Design (DDD) inside the application core, hexagonal architecture (ports and adapters), safe Rust, local-first operation.

This roadmap defines the intended implementation order, dependencies, deliverables, and exit criteria. A phase is not complete merely because code compiles: its architectural, behavioral, test, and documentation gates must also pass. Re-plan through an ADR when evidence requires a structural change.

## Planning and tracking policy

The project plan, phase breakdown, progress states, decisions, and open work are maintained as Markdown files in this Git repository (primarily this roadmap, arc42, the specification, and ADRs). **Do not create or maintain GitHub Issues for project planning, tasks, bugs, phase tracking, or backlog management.** Update the relevant Markdown document in the same PR as the implementation change. Pull requests remain the mechanism for proposing, reviewing, and delivering code; they are not a reason to mirror the plan into Issues. Keep the plan usable directly from a repository checkout and readable in GitHub's Markdown renderer.

## Delivery principle: vertical, UI-testable increments

After the architecture/core foundation, user-visible capabilities MUST be delivered as vertical slices. Each slice connects the real capability through its adapter, core use case, desktop composition root, and Slint UI in the same PR (or a clearly linked, immediately following PR when a dependency gate makes that impossible). Do not accumulate completed backend features that cannot be exercised from the application until a late integration phase.

Every slice must:
- Expose the implemented capability in the running desktop UI, with real state and typed error feedback; never use a mock success state as evidence of production integration.
- Include a concise manual UI test procedure and record which OS/device combinations were actually tested. CI/UI snapshots do not replace real-device evidence.
- Keep not-yet-implemented controls visibly disabled or explicitly marked as unavailable; no dead buttons, fake device selectors, or misleading “ready” status.
- Include automated core/adapter/UI tests and preserve hexagonal dependency direction, DDD invariants, safe-Rust rules, and non-blocking UI.
- Update this roadmap and relevant arc42/specification sections in the same PR.

Track four separate evidence states: **implemented**, **automated-tested**, **UI-integrated**, and **real-device-validated**. Never conflate them.

## Delivery rules applying to every phase

1. Keep the domain model, ubiquitous language, invariants, and application use cases in the core. Define ports in application-owned code.
2. Apply DDD pragmatically: identify entities/value objects and aggregate boundaries from real invariants; keep domain logic framework-free; introduce domain events and bounded contexts only when justified.
3. Implement platform, UI, media, and storage integrations as adapters. The desktop executable is the only composition root and injects adapter implementations.
4. Core must not depend on Slint, CPAL, PipeWire, OS APIs, codecs, muxers, or filesystem implementation types. No adapter-to-adapter coupling.
5. No first-party `unsafe`, raw pointers, custom FFI, unbounded media queues, or blocking UI/audio callbacks.
6. Translate external types/errors at adapter boundaries. Use typed errors and owned data across asynchronous boundaries.
7. Test domain invariants and aggregate behavior directly; test core use cases with fakes; adapter contracts independently; platform behavior on real supported OS/device combinations.
8. Update the relevant arc42 sections and add an ADR for significant decisions in the same PR.
9. Do not claim a platform, mode, or backend as supported until its real-device and packaging gates pass.

## Phase overview

| Phase | Scope | Status | Depends on |
|---|---|---|---|
| 0 | Architecture baseline, capture contracts, bounded audio worker transport | Complete — PR #12 | — |
| 1 | Core use cases, inbound/outbound ports, recording session orchestration | Implemented — PR #13 merged; hosted CI validation tracked independently | 0 |
| 2 | Media pipeline evaluation and backend decision | In progress — bounded producer/consumer queue and native EOS/sink/timeout validation implemented; backend decision recorded as continued deferral; independent playback, hardware, packaging and license evidence remain open | 0, 1 (port requirements) |
| 3 | Transactional local storage adapter and recovery contract | In progress — filesystem adapter slice merged in PR #16; failure-injection and durability gates remain open | 1 |
| 4 | Microphone adapter behind the audio output port | Planned | 1, 2 |
| 5 | Screen capture adapters: Wayland first, Windows | Planned | 1, 2 |
| 6 | Camera adapter and screen + camera composition | Planned | 1, 2, 5 |
| 7 | Desktop composition root and UI integration | Cross-cutting: integrate each capability incrementally; no late big-bang UI phase | 1–6 |
| 8 | End-to-end hardening, performance, packaging and release gates | Planned | 3–7 |

Phases may be delivered as smaller PRs. Parallel work is allowed only where the port contracts and dependencies are already stable. Phase 7 is not a deferred integration bucket: the composition root and UI are extended alongside Phases 4–6 so each finished capability can be tried immediately.

## Phase 0 — Architecture baseline and transport contracts

**Current PR: #12**

Deliverables:
- Backend-neutral owned audio/video data contracts and validation.
- Bounded worker-level audio block FIFO with explicit full-queue rejection.
- Callback/worker separation: callback-facing preallocated SPSC ring remains separate.
- Binding hexagonal architecture and DDD rules, ubiquitous language, initial Recording bounded context, and arc42 documentation policy.
- This implementation roadmap and ADR trail.

Exit criteria:
- [ ] PR checks pass on Linux and Windows plus dependency audits.
- [ ] Unit tests cover block validation, queue bounds, FIFO behavior, format mismatch, and overflow accounting.
- [ ] No first-party unsafe code or new unreviewed dependencies.
- [ ] All 12 arc42 sections remain present and current.
- [ ] Contributor instructions enforce ports-and-adapters and DDD for every feature.
- [ ] Domain vocabulary and initial Recording model are documented; domain invariants remain independent of adapters.

## Phase 1 — Core use cases and ports

**PR #13 merged:** completes the core contract and orchestration slice for Phase 1. Production adapters and durable media storage remain later phases; the session repository contract is defined here, while its durable adapter is intentionally deferred.

Deliverables:
- Define application-owned inbound port/use cases for prepare, start, pause, resume, stop, cancel, and state query; define event publication and coalesced metrics snapshot contracts as outbound ports.
- Define separate outbound ports for screen, camera, microphone, media pipeline, storage, monotonic clock, settings, session metadata repository, event publication, and metrics snapshots.
- Introduce a Recording session aggregate with explicit identity, configuration, lifecycle state, cancellation, finalization outcome, and artifact publication only after successful finalization.
- Make the core lifecycle authoritative; adapters report facts and never decide application state. Publish typed lifecycle/source-loss events and expose coalesced metrics snapshots.
- Add fake resource adapters and deterministic tests for legal/illegal transitions, cancellation, orchestration order, duplicate session identity, and finalization failures. Keep device-loss outcomes typed at the source-port boundary.

Exit criteria:
- [x] Core has no concrete UI, OS, capture, media, or filesystem dependency.
- [x] Recording aggregate owns lifecycle transitions and rejects illegal operations.
- [x] Configuration invariants prevent optional microphone duplication in microphone-only mode.
- [x] Finalized artifact is exposed only after successful resource finalization.
- [x] Application use cases are defined by an inbound port and depend on core-owned outbound contracts.
- [x] Use cases can be exercised without hardware or native media runtimes.
- [x] Every port has a documented ownership, threading, error, cancellation, and shutdown contract in its API documentation.
- [x] Core dependency boundary is explicit: only the backend-neutral capture-contract crate is a direct dependency; adapters depend inward on core ports.
- [x] arc42 sections 4–6 and relevant ADRs updated.

## Phase 2 — Media pipeline evaluation and decision

**Merged baseline:** PR #14 introduced the isolated GStreamer dual-stream PoC; PR #15 added repeatable model tests and the real Linux EOS integration baseline.

**This implementation slice:** adds a fixed-capacity producer-to-pipeline-worker queue, non-blocking producer sends, counted video drop-newest, fatal audio overload, queue metrics, concurrent queue tests, sink-error and EOS-timeout integration tests. It records the candidate comparison and continued backend deferral in [ADR-0015](adr/ADR-0015-media-backend-evaluation-decision.md). GStreamer remains outside the production workspace.

Deliverables:
- Compare GStreamer, FFmpeg Rust bindings, and pure-Rust candidates against safe API boundaries, Windows/Linux availability, codec/container coverage, native footprint, licensing, packaging, and offline operation. The current evidence supports continued deferral, not production selection.
- Keep the synthetic VP8/Vorbis-to-Matroska pipeline isolated. It is a diagnostic format, not approval of the final MP4/H.264/AAC profile.
- Maintain hardware-free reference tests for per-stream timestamps, sequence continuity, audio sample positions, queue capacity, and terminal states.
- Exercise the real GStreamer pipeline in Linux CI for pipeline-wide EOS, non-empty output, filesink failure, and bounded EOS timeout. Encoder-specific injection, process interruption, and crash recovery remain external gates.
- Use a real fixed-capacity producer/consumer channel between synthetic producer and pipeline worker. Producer sends are non-blocking; video overload is counted and dropped, audio overload fails explicitly, and queue high-water/rejection metrics are surfaced.
- Keep all media work off UI and real-time callback threads. The terminal marker is control data and is retried on the synthetic worker; production control-path independence still requires sustained-load validation.
- Record CPU/RSS/startup/throughput/A/V drift/stop-to-finalize measurements only from documented hardware/software runs; queue counters are emitted by the PoC but do not substitute for those measurements.
- Keep the native runtime/plugin inventory and exact redistribution review open until a concrete production profile is proposed.

Exit criteria:
- [ ] Representative sample recordings play in at least two independent players on each tested OS.
- [ ] Output target formats are validated separately; diagnostic Matroska/VP8/Vorbis does not approve MP4/H.264/AAC.
- [x] Reference-model tests cover per-stream timestamp ordering, contiguous sequence numbers, audio sample-position continuity, queue-model bounds and terminal-state combinations; these do not count as pipeline integration evidence.
- [x] Linux CI integration test verifies real synthetic audio/video pipeline EOS and non-empty output.
- [ ] GStreamer integration tests verify encoder-specific errors, interruption and incomplete-output cleanup; filesink error and bounded EOS timeout are covered.
- [x] A fixed-capacity producer/consumer queue is used by the PoC; producer sends are non-blocking, video drops are counted, audio overload fails explicitly, and concurrent producer/full-queue behavior is tested.
- [ ] Sustained overload, long-run audio-gap handling, and control-path independence are verified under representative load.
- [ ] Exact dependency/native plugin inventory and redistribution licenses are reviewed for a proposed production profile.
- [x] No unsafe code is introduced into first-party crates; the PoC crate forbids unsafe code.
- [x] Candidate comparison and continued defer decision are recorded in ADR-0015 and arc42 sections 5, 7, 8, 9, and 11.
- [ ] Measured CPU, process RSS, startup, throughput, queue high-water marks, A/V drift and stop-to-finalize latency are recorded with hardware/software context.


## Phase 3 — Transactional local storage

**Merged slice:** PR #16 introduces the isolated filesystem adapter for `RecordingStoragePort`; see [ADR-0014](adr/ADR-0014-transactional-filesystem-storage.md) and `crates/storage/README.md`. The filesystem adapter is now instantiated by the desktop composition root for output-directory validation and non-destructive orphan discovery. The UI displays the default directory and recovery scan results; native directory selection remains open because no cross-platform dialog adapter has been approved. This does not approve a production media backend.

Deliverables:
- Storage port and filesystem adapter for exclusive temporary output, collision-safe destination naming, flush/close, no-replace publication, and recovery discovery.
- Explicit behavior for disk full, permission errors, path invalidation, process interruption, and existing files.
- No overwrite by default; no success before finalization and close.
- Media writer integration and composition remain blocked on a production media decision and are not claimed by this slice. The prepare-recording action remains visibly disabled until a real media backend and capture path are integrated.

Exit criteria:
- [ ] Fault-injected tests cover write, flush, close, publication, and disk-full failures on supported filesystems.
  - Note: adapter-level fault-injection coverage exists in `crates/storage` (a private `FileOperations` seam injects StorageFull/PermissionDenied/NotFound/other errors into create, flush, sync, close, hard-link, remove, and directory scan; see ADR-0014). Media-writer `write` failures belong to the not-yet-selected media adapter. Real-filesystem runs on NTFS, ext4, and btrfs (including genuine disk-full) remain open, so this box stays unchecked.
- [x] Existing recordings are never silently overwritten by the adapter's no-replace publication strategy.
- [x] Recovery discovery reports orphaned candidates without destructive cleanup.
- [x] Core depends only on the storage port; the adapter depends inward on core.
- [x] Desktop composition root uses the storage adapter to validate the default output path and run non-destructive orphan discovery; the UI displays results.
- [ ] Native output-directory selection is implemented through an approved cross-platform dialog adapter.
- [ ] Desktop/media integration and closed-writer hand-off are verified.
- [ ] Filesystem durability and real power-loss recovery are validated on supported OS/filesystems.

## Phase 4 — Production microphone adapter and first usable UI slice

This phase must end with microphone-only capture and optional microphone input selectable and exercisable from the running desktop application on each validated platform. The microphone selector must not be a decorative placeholder.

Deliverables:
- Promote the isolated CPAL evaluation only after dependency/security review and real-device tests.
- Implement the core-owned microphone port as an outbound adapter.
- Enumerate and select devices explicitly; negotiate sample formats/rate/channels; handle unplug, permission/device errors, and stream shutdown.
- Keep callback work bounded and non-blocking. Use the preallocated SPSC ring in callback context; drain and normalize on a worker; publish AudioBlocks through the bounded worker queue.
- Propagate overrun/discontinuity as typed session events and degraded/failed outcomes.

Exit criteria:
- [ ] Linux ALSA/PipeWire and Windows WASAPI device tests pass on documented hardware.
- [ ] Format negotiation and conversion are explicit; unsupported formats fail clearly.
- [ ] Long-run audio continuity, timestamps, overrun and device-loss tests pass.
- [ ] Callback performs no allocation, blocking, I/O, logging, or encoding.
- [ ] Adapter conforms to core port tests and does not expose CPAL types.

## Phase 5 — Screen capture adapters and UI slice

This phase must end with OS-mediated display/window selection and screen capture accessible from the running desktop application. Preserve the already-working microphone UI path.

Deliverables:
- Wayland adapter using user-mediated XDG Desktop Portal selection and PipeWire stream; retain a safe API boundary and explicit session ownership.
- Windows adapter using user-mediated Windows capture APIs through a reviewed safe wrapper.
- Source enumeration/selection, capabilities, frame timestamps, stride/format validation, cancellation, session closure, and disconnect handling.
- X11 remains a separate optional future decision; do not imply support.

Exit criteria:
- [ ] Real KDE and GNOME Wayland tests pass.
- [ ] Windows 10/11 display/window capture tests pass.
- [ ] User cancellation and permission denial are distinct outcomes.
- [ ] Frame buffers are owned/validated; queues bounded; drops counted.
- [ ] No bypass of OS permission model and no first-party unsafe code.

## Phase 6 — Camera, picture-in-picture and UI slice

This phase must end with webcam-only and screen-plus-webcam modes accessible from the running desktop application. Preserve the microphone and screen paths.

Deliverables:
- Camera port adapter with explicit device/format negotiation and hot-unplug handling.
- Composition stage places camera frames into screen frames according to a deterministic layout contract.
- Timestamp alignment, resize/color conversion, and bounded buffering are explicit.
- No camera framework types cross the port.

Exit criteria:
- [ ] Webcam-only and screen + webcam modes work on each claimed OS.
- [ ] Device loss and unsupported formats are handled visibly.
- [ ] PIP layout and A/V synchronization pass integration tests.
- [ ] Resource use remains bounded during long sessions.

## Phase 7 — Continuous desktop composition and UI integration

This is a cross-cutting activity, not a final integration phase. It starts with the first user-visible slice and continues in every subsequent PR.

Deliverables:
- Desktop executable constructs and injects each newly implemented adapter into core use cases as soon as its contracts and evidence gates permit.
- Slint UI acts only as an inbound adapter: typed user commands in, state/progress/events out.
- Every completed capability is immediately accessible from the running app; incomplete capabilities are explicitly unavailable rather than represented by non-functional controls.
- UI remains responsive; no capture, encoding, or filesystem work on UI thread.
- Preferences are validated and stored through a settings port.

Exit criteria:
- [ ] No UI-to-concrete-adapter shortcuts.
- [ ] No global service locator or concrete backend construction in core.
- [ ] Every completed capability has an end-to-end UI path and manual test procedure.
- [ ] UI distinguishes implemented, automated-tested, UI-integrated, and real-device-validated.
- [ ] Accessibility, cancellation, error states, and progress are covered by UI tests.

## Phase 8 — End-to-end hardening and release

Deliverables:
- Full scenario matrix for four modes, optional microphone, pause/resume/stop/cancel, unplug, disk-full, permission denial, and recovery.
- Long-duration and stress tests for memory, queue occupancy, audio continuity, sync drift, CPU, and stop-to-finalize latency.
- Offline/network observation and privacy review.
- Reproducible Windows/Linux packaging, clean-machine installation tests, dependency and license audits, release notes.

Exit criteria:
- [ ] Every advertised OS/mode has real-device evidence.
- [ ] No false success on incomplete or corrupted output.
- [ ] Resource and performance targets are measured and documented with hardware/software context.
- [ ] Security/dependency audit passes.
- [ ] arc42 and ADRs match shipped behavior; unsupported capabilities are explicitly listed.

## Change control

A phase may be split, reordered, or deferred only with documented evidence. If the change alters dependency direction, lifecycle ownership, trust boundaries, media time model, backend selection, or platform support, create a new ADR and update arc42 before or alongside implementation. The hexagonal dependency rule and 12-section arc42 requirement cannot be waived by phase planning.
