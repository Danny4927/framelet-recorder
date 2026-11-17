# Framelet implementation roadmap

**Status:** Binding delivery plan  
**Last updated:** 17 November 2026  
**Architecture authority:** [arc42](arc42.md), [ADR index](adr/README.md)  
**Mandatory implementation style:** hexagonal architecture (ports and adapters), safe Rust, local-first operation.

This roadmap defines the intended implementation order, dependencies, deliverables, and exit criteria. A phase is not complete merely because code compiles: its architectural, behavioral, test, and documentation gates must also pass. Re-plan through an ADR when evidence requires a structural change.

## Delivery rules applying to every phase

1. Keep all application use cases and domain/application contracts in the core. Define ports in application-owned code.
2. Implement platform, UI, media, and storage integrations as adapters. The desktop executable is the only composition root and injects adapter implementations.
3. Core must not depend on Slint, CPAL, PipeWire, OS APIs, codecs, muxers, or filesystem implementation types. No adapter-to-adapter coupling.
4. No first-party `unsafe`, raw pointers, custom FFI, unbounded media queues, or blocking UI/audio callbacks.
5. Translate external types/errors at adapter boundaries. Use typed errors and owned data across asynchronous boundaries.
6. Test core use cases with fakes; adapter contracts independently; platform behavior on real supported OS/device combinations.
7. Update the relevant arc42 sections and add an ADR for significant decisions in the same PR.
8. Do not claim a platform, mode, or backend as supported until its real-device and packaging gates pass.

## Phase overview

| Phase | Scope | Status | Depends on |
|---|---|---|---|
| 0 | Architecture baseline, capture contracts, bounded audio worker transport | Current PR | — |
| 1 | Core use cases, inbound/outbound ports, recording session orchestration | Planned | 0 |
| 2 | Media pipeline evaluation and backend decision | Planned | 0, 1 (port requirements) |
| 3 | Transactional local storage adapter and recovery contract | Planned | 1 |
| 4 | Microphone adapter behind the audio output port | Planned | 1, 2 |
| 5 | Screen capture adapters: Wayland first, Windows | Planned | 1, 2 |
| 6 | Camera adapter and screen + camera composition | Planned | 1, 2, 5 |
| 7 | Desktop composition root and UI integration | Planned | 1–6 (incremental wiring only) |
| 8 | End-to-end hardening, performance, packaging and release gates | Planned | 3–7 |

Phases may be delivered as smaller PRs. Parallel work is allowed only where the port contracts and dependencies are already stable.

## Phase 0 — Architecture baseline and transport contracts

**Current PR: #12**

Deliverables:
- Backend-neutral owned audio/video data contracts and validation.
- Bounded worker-level audio block FIFO with explicit full-queue rejection.
- Callback/worker separation: callback-facing preallocated SPSC ring remains separate.
- Binding hexagonal architecture rules and arc42 documentation policy.
- This implementation roadmap and ADR trail.

Exit criteria:
- [ ] PR checks pass on Linux and Windows plus dependency audits.
- [ ] Unit tests cover block validation, queue bounds, FIFO behavior, format mismatch, and overflow accounting.
- [ ] No first-party unsafe code or new unreviewed dependencies.
- [ ] All 12 arc42 sections remain present and current.
- [ ] Contributor instructions enforce ports-and-adapters for every feature.

## Phase 1 — Core use cases and ports

Deliverables:
- Define application-owned inbound ports/use cases: prepare, start, pause, resume, stop, cancel, and state/metrics subscription.
- Define outbound ports for screen, camera, microphone, media pipeline, storage, clock, and settings as required by concrete use cases.
- Introduce a recording session abstraction with explicit ownership, cancellation, idempotent stop, and typed outcomes.
- Make the core lifecycle authoritative; adapters report events and never decide application state.
- Add fake adapters and deterministic tests for legal/illegal transitions, cancellation, device loss, stop ordering, and finalization failures.

Exit criteria:
- [ ] Core has no concrete UI, OS, capture, media, or filesystem dependency.
- [ ] Use cases can be exercised without hardware or native media runtimes.
- [ ] Every port has a documented ownership, threading, error, cancellation, and shutdown contract.
- [ ] Architecture/dependency checks prevent inward dependency violations.
- [ ] arc42 sections 4–6 and relevant ADRs updated.

## Phase 2 — Media pipeline evaluation and decision

Deliverables:
- Compare viable media runtimes and/or libraries against Windows/Linux support, Rust integration, codecs/containers, plugin footprint, safe API availability, latency, memory, licensing, packaging, and offline operation.
- Build a narrow isolated PoC that accepts timestamped owned audio/video blocks, encodes/muxes a local file, and finalizes deterministically.
- Measure startup, CPU/RSS, queue occupancy, A/V sync, stop-to-finalize latency, and failure recovery.
- Record the selected approach (or continued deferral) in an ADR. Do not let the candidate runtime leak through core ports.

Exit criteria:
- [ ] Representative sample recordings play in independent players on each tested OS.
- [ ] Finalization and injected failure tests pass.
- [ ] Dependency/native plugin inventory and redistribution licenses are reviewed.
- [ ] No unsafe code is introduced into first-party crates.
- [ ] Selection/defer decision is recorded in an ADR and arc42 sections 5, 7, 8, 11 updated.

## Phase 3 — Transactional local storage

Deliverables:
- Storage port and filesystem adapter for exclusive temporary output, collision-safe destination naming, flush/close, atomic publication where supported, and recovery metadata.
- Explicit behavior for disk full, permission errors, path invalidation, process interruption, and existing files.
- No overwrite by default; no success before finalization and close.

Exit criteria:
- [ ] Fault-injected tests cover write, flush, close, rename/publication, and disk-full failures.
- [ ] Existing recordings are never silently overwritten.
- [ ] Recovery behavior is documented and tested.
- [ ] Core depends only on the storage port.

## Phase 4 — Production microphone adapter

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

## Phase 5 — Screen capture adapters

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

## Phase 6 — Camera and picture-in-picture

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

## Phase 7 — Desktop composition root and UI integration

Deliverables:
- Desktop executable constructs and injects selected adapters into core use cases.
- Slint UI acts only as an inbound adapter: typed user commands in, state/progress/events out.
- UI remains responsive; no capture, encoding, or filesystem work on UI thread.
- Preferences are validated and stored through a settings port.

Exit criteria:
- [ ] No UI-to-concrete-adapter shortcuts.
- [ ] No global service locator or concrete backend construction in core.
- [ ] All MVP modes can be exercised through the application use cases.
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
