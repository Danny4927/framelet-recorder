# Framelet — arc42 Software Architecture

**Version:** 0.2 — capture/media contracts and initial Wayland PoC  
**Date:** 3 November 2026  
**Status:** Proposed architecture; capture and media technology choices remain subject to PoC gates.

> This document describes the intended MVP architecture. The repository currently contains a Slint shell, a minimal capture contract, and a lifecycle model. Native capture, media processing, storage finalization, and device integration are not implemented.

## 1. Introduction and Goals

Framelet is a local-first desktop recorder for four modes: screen (optional microphone), microphone-only, webcam (optional microphone), and screen plus webcam picture-in-picture (optional microphone). No system audio, network, upload, account, cloud service, collaboration, AI, editing, in-app playback, or macOS support.

| Priority | Quality goal | Acceptance scenario |
|---|---|---|
| 1 | Privacy | Capture starts only after explicit user action and OS-mediated selection/permission; no network traffic |
| 2 | Reliability | Stop drains accepted buffers and finalizes output before success is reported |
| 3 | Safety | First-party Rust crates forbid unsafe code |
| 4 | Responsiveness | UI never blocks on device enumeration, capture, encoding, or disk I/O |
| 5 | Bounded resources | Every producer/consumer boundary has bounded buffering and an overload policy |
| 6 | Portability | OS-specific capture is isolated behind application-owned interfaces |
| 7 | Maintainability | Core use cases are testable without physical devices |

Performance numbers in SPECIFICATION.md are targets to measure, not guarantees.

Stakeholders: end users (predictable private recordings), maintainers (auditable and testable code), security reviewers (explicit trust boundaries), and packagers (reproducible native dependencies).

## 2. Architecture Constraints

- Windows 10/11; Linux Wayland first-class; X11 separate and not supported until validated; macOS out of scope.
- Exactly four MVP modes; optional microphone where specified; no system audio.
- Local files only; offline recording; OS-mediated permissions; never bypass compositor/portal controls.
- Rust Edition 2024. Every first-party crate must use #![forbid(unsafe_code)]. No raw pointers, custom FFI, unsafe trait implementations, or lint exceptions.
- Third-party dependencies may contain unsafe internals, but require review of unsafe/FFI boundaries, advisories, license, maintenance, transitive dependencies, native libraries, and platform parity.
- UI thread is non-blocking; audio callbacks are real-time safe; queues are bounded.
- Monotonic timestamps drive media synchronization; validate external dimensions and use checked arithmetic.
- If a capability cannot be implemented through a suitable safe API, defer it or select another backend; never weaken the safety rule.

## 3. Context and Scope

Framelet is one desktop process with no server-side counterpart and no internet requirement.

~~~mermaid
flowchart LR
  User -->|mode, source, controls| UI[Framelet UI]
  UI --> Core[Application core]
  Core -->|capture request| OS[OS picker and permissions]
  OS --> Capture[Capture adapters]
  Capture -->|timestamped media| Core
  Core --> Media[Media pipeline]
  Media --> Storage[Local filesystem]
  Storage -->|completed path| UI
~~~

External interfaces:
- Windows Graphics Capture / OS media APIs: user-selected screen/window and device access through a reviewed safe adapter.
- XDG Desktop Portal + PipeWire: Wayland selection, permission, and streams; portal consent is authoritative.
- X11: separate future adapter with a different security model.
- Camera/microphone devices: hot-unplug, unsupported formats, and permission changes are expected.
- Local filesystem: paths and all I/O outcomes are fallible.
- Native media runtime: candidate only; runtime/plugin footprint and loading policy require review.

No telemetry, update checks, upload, remote control, or cloud processing is in scope.

## 4. Solution Strategy

1. Modular monolith: one desktop process and Rust workspace; no daemon or distributed architecture.
2. Ports and adapters: core depends on application-owned interfaces, not platform crates.
3. Core-owned lifecycle: one authority coordinates prepare, record, pause, resume, stop, cancel, and finalize.
4. Message-driven boundaries: UI sends commands and receives state/progress events; workers publish owned media blocks through bounded transport.
5. One monotonic media timeline with a common recording origin.
6. Local transactional output: exclusive temporary file, successful finalization, then collision-safe publication.
7. Fail visibly: count video drops; detect/report audio loss, disconnects, and finalization errors.
8. Safe Rust boundary; third-party unsafe internals are reviewed risks.
9. Capture/media dependencies remain candidates until real-device PoCs and packaging/security review pass.

## 5. Building Block View

The detailed backend-neutral media boundary is specified in [Capture and media contracts](capture-and-media-contracts.md). It defines source identity/capabilities, owned video/audio blocks, validation, clock mapping, events/errors, bounded transport, and cancellation.

### 5.1 Application overview

~~~mermaid
flowchart TB
  Desktop[Desktop executable / composition root]
  Desktop --> UI[Slint UI and UI adapter]
  Desktop --> Core[Core: use cases and lifecycle]
  Core --> Ports[Application-owned ports]
  Ports --> Capture[Platform capture adapters]
  Ports --> Media[Media pipeline adapter]
  Ports --> Storage[Storage adapter]
  Capture --> OS[OS and devices]
  Media --> Runtime[Native media runtime — candidate]
  Storage --> FS[Local filesystem]
~~~

### 5.2 Responsibilities

| Component | Responsibility | Must not do |
|---|---|---|
| apps/desktop | Entry point, Slint window, wiring, UI event translation | Capture, encode, blocking I/O, own lifecycle rules |
| crates/core | Use cases, validated commands, state machine, orchestration | Depend on Slint or concrete platform/media crates |
| crates/capture-api | Safe source/frame/audio contracts and typed errors | Expose raw pointers, native handles, unbounded queues |
| capture-screen (future) | Screen selection and acquisition per OS | Bypass OS permissions |
| capture-camera (future) | Camera enumeration, negotiation, acquisition | Hide disconnects or silently switch devices |
| capture-audio (future) | Microphone input | Block/allocate unpredictably in audio callback |
| media-pipeline (future) | Timeline, compositing, encoding, muxing, finalization | Own UI state or silently discard audio |
| storage (future) | Exclusive temp files, collision-safe naming, recovery | Overwrite files or report success before close |
| config/platform (future) | Validated preferences and narrowly scoped OS integration | Become catch-all business logic |

Create crates only when real code justifies a boundary. No empty placeholder members.

### 5.3 Dependency direction

The desktop executable is the composition root. Core depends on contracts, not Slint, GStreamer, CPAL, nokhwa, PipeWire, or Windows API crates. Adapters implement ports and are wired by desktop. Core unit tests use fakes and do not need capture devices.

### 5.4 Current state versus target

| Area | Status |
|---|---|
| Slint shell | Present; scaffold only |
| capture-api | Safe source descriptors, validated packed BGRA/RGBA video frames, and interleaved audio blocks implemented; streaming transport remains open |
| core | Mode enum and transition predicate; orchestration incomplete |
| Native capture | Not implemented |
| Media pipeline/compositing/encoding | Not implemented |
| Storage/recovery | Not implemented |
| Platform packaging | Not implemented |

## 6. Runtime View

### 6.1 Prepare and start

~~~mermaid
sequenceDiagram
  actor User
  participant UI
  participant Core
  participant OS
  participant Capture
  participant Storage
  participant Media
  User->>UI: Select mode and press prepare
  UI->>Core: PrepareRecording
  Core->>OS: Request source selection and permission
  OS-->>Capture: Authorized source/session
  Capture-->>Core: Capabilities
  Core->>Storage: Create exclusive temporary output
  Storage-->>Core: Output ready
  Core->>Media: Prepare timeline and pipeline
  Media-->>Core: Ready
  Core-->>UI: Ready event
  User->>UI: Confirm start
  UI->>Core: Start
  Core->>Capture: Start producers
  Capture-->>Media: Owned timestamped blocks
  Media-->>Storage: Encoded/muxed data
  Core-->>UI: Recording state and metrics
~~~

Exact picker invocation is adapter-dependent. Cancellation must not start capture or create a falsely completed output.

### 6.2 Stop and finalize

~~~mermaid
sequenceDiagram
  actor User
  participant Core
  participant Capture
  participant Media
  participant Storage
  User->>Core: Stop
  Core->>Capture: Stop all producers
  Capture-->>Core: Producers stopped
  Core->>Media: Drain accepted buffers and finalize
  Media-->>Storage: Final packets and container trailer
  Storage-->>Core: Flush, close, publish success
  Core-->>User: Completed(path)
~~~

Any required finalization failure produces a failure/recovery result, never success. Stop is idempotent. Discard is a separate explicit use case.

### 6.3 Pause/resume and failures

Pause is coordinated across sources and pipeline. The core records monotonic pause boundaries and excludes paused duration from output timestamps. Exact semantics must be validated with the selected backend. Adapters emit typed permission, cancellation, unsupported-format, disconnect, and capture errors; the core decides whether to continue or stop. Adapters never silently substitute the selected source.

## 7. Deployment View

One installed desktop process; no daemon or helper service.

- Windows: executable plus explicitly packaged and reviewed runtime dependencies.
- Linux Wayland: executable plus documented portal/PipeWire integration and runtime dependencies.
- Linux X11: separate adapter/package validation; no implied Wayland parity.
- Output and settings stay in user-owned local directories.

Native runtime/plugin inventory must be explicit and reproducible. Do not load arbitrary plugins from user-writable paths. Test packages on clean machines; recording must work offline.

## 8. Cross-cutting Concepts

### Ownership and concurrency
Use owned media blocks at asynchronous boundaries unless a safe lifetime-bound abstraction is demonstrably better. No borrowed view may outlive its source buffer. Worker cancellation and shutdown are explicit and observed.

### Backpressure
Every queue has a fixed capacity and policy. Video may drop stale frames to bound latency, but drops are counted and surfaced. Audio loss is never silent. Control events cannot be starved by media payloads. UI metrics are coalesced. Queue capacities remain open until formats and throughput are measured.

### Time and synchronization
Use a monotonic clock and common origin. Preserve source timestamps where possible; track audio sample positions and video sequence numbers. Handle discontinuities, variable frame rates, pause segments, and drift. Wall-clock time may label files but never drives A/V sync.

### Security and privacy
Explicit user action and OS-mediated permissions; no network or telemetry. Never log captured media or unnecessary device identifiers. Validate dimensions, stride, formats, lengths, arithmetic, and paths. Restrict output permissions where supported. Review unsafe internals, FFI, advisories, licenses, native libraries, dynamic loading, and plugins.

### Error handling
Typed errors cross module boundaries and become actionable UI messages. Preserve diagnostic causes without exposing sensitive content. Distinguish cancellation from denial and backend failure. No panic in production paths.

### Testing
Unit tests cover state/use cases, timestamps, settings, paths, checked buffer calculations. Contract tests cover adapter lifecycle, cancellation, disconnect, format negotiation, and queue bounds. Real-device integration tests cover every claimed OS/mode. Visual regression is separate from capture correctness. Long-run tests measure memory, audio continuity, sync, and finalization.

## 9. Architecture Decisions

See [ADR index](adr/README.md).

| ADR | Decision | Status |
|---|---|---|
| ADR-0001 | Modular monolith and ports-and-adapters | Accepted |
| ADR-0002 | Core-owned recording lifecycle | Accepted |
| ADR-0003 | Safe Rust and platform adapters | Accepted |
| ADR-0004 | Defer media runtime selection until PoC | Accepted (defer) |
| ADR-0005 | Owned media blocks and explicit clock contracts | Accepted for PoC |
| ADR-0006 | Start Wayland portal PoC with lamco-wayland | Accepted for evaluation only |

## 10. Quality Requirements

| ID | Stimulus | Expected response | Verification |
|---|---|---|---|
| Q-01 | User cancels source picker | Return to ready/idle; no capture starts | Adapter contract test |
| Q-02 | User stops recording | Producers stop, accepted buffers drain, output finalizes before success | Integration test and inspect file |
| Q-03 | Camera unplugged | Typed loss event; user informed; safe stop or explicitly supported continuation | Real-device test |
| Q-04 | Encoder slower than video producer | Queue remains bounded; drops counted and surfaced | Synthetic stress test |
| Q-05 | Audio overrun | Gap detected; output marked degraded/failed | Transport and device tests |
| Q-06 | Disk full | Stop safely; attempt finalization/recovery; no false success | Fault-injected storage test |
| Q-07 | 60-minute recording | Bounded memory and stable timestamps/sync | Long-run device test |
| Q-08 | Offline operation | Recording remains usable; no network attempted | Offline/network observation |
| Q-09 | High-rate metrics | UI responsive; metrics coalesced | UI load test |

SPECIFICATION.md contains performance targets. Publish measurements only with hardware, OS, backend, resolution, frame rate, codecs, duration, and encoder mode.

## 11. Risks and Technical Debt

| Risk | Impact | Mitigation / evidence required |
|---|---|---|
| Safe wrapper lacks required OS behavior | Platform capability blocked under no-unsafe policy | Test public safe APIs early; defer rather than add unsafe glue |
| GStreamer runtime/plugin footprint | Package complexity and support burden | Minimal plugin inventory and clean-machine PoC |
| Wayland desktop differences | Uneven Linux behavior | GNOME and KDE tests; portal session/cancel handling |
| Audio overload | Gaps and sync errors | Bounded ring buffer, counters, stress/device tests |
| Pause semantics differ | Timestamp discontinuities | Common timeline contract and conformance tests |
| MP4 crash recovery limited | Partial file may be unusable | Temp output, documented recovery, evaluate fragmented formats |
| Device formats vary | Unsupported mode or expensive conversion | Explicit negotiation and capability reporting |
| Plugin discovery risk | Supply-chain/runtime loading risk | Explicit plugin allowlist and packaged paths |
| Capture API too small for streaming | Expensive API rework | Design ownership and event contract during PoC |

Open questions: safe Windows capture wrapper; safe Wayland portal/PipeWire API; CPAL callback/device-loss behavior; nokhwa cross-platform quality; GStreamer versus alternatives; owned pixel/audio formats; measured queue capacities.

## 12. Glossary

| Term | Meaning |
|---|---|
| Adapter | Platform/library-specific implementation of an application-owned port |
| Backpressure | Preventing a fast producer from overwhelming a slower consumer |
| Capture source | User-selected display, window, camera, or microphone |
| Core | Application layer owning use cases and recording lifecycle |
| Muxing | Combining encoded streams into a media container |
| Monotonic clock | Clock suitable for elapsed time that does not move backwards |
| Port | Application-owned interface for an external capability |
| PipeWire | Linux multimedia transport for portal-provided streams |
| XDG Desktop Portal | Desktop-mediated interface for user-controlled resource access |
