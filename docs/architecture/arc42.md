# Framelet — arc42 Software Architecture

**Version:** 0.12 — Native Windows title-bar styling  
**Date:** 17 November 2026  
**Status:** Proposed architecture; capture and media technology choices remain subject to PoC gates.

> This document describes the intended MVP architecture. The repository currently contains a Slint shell, safe capture data contracts, an isolated Wayland frame PoC, an isolated CPAL microphone PoC, a bounded worker-level audio queue, the initial Recording session aggregate and use cases, core-owned source/media/storage/clock/settings/repository/event/metrics ports, and an isolated GStreamer media evaluation PoC. No production media backend has been selected. Native capture has not been integrated into the production application. The filesystem storage adapter implements the core storage port as an isolated crate; it is not yet composed with a production media writer. Its latest hardening adds collision-safe temporary-name reservation retries and regression coverage for existing-file collisions, aborted candidates, and publication behavior. The isolated GStreamer PoC now uses a bounded producer-to-pipeline-worker queue and covers EOS, filesink failure, and EOS timeout on Linux CI. Production media selection, independent playback, encoder/interruption failure injection, sustained-load measurements, native redistribution review, and real-device validation remain open.

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
- **Binding architecture:** hexagonal architecture (ports and adapters) applies to the whole application. Core owns use cases, domain/application types, and ports. Concrete adapters depend inward on those contracts. The desktop executable is the only composition root. No production feature may bypass this boundary.
- **Binding domain model:** apply DDD inside the core. Maintain a ubiquitous language; encode business invariants in domain entities/value objects/aggregates where justified; keep the domain independent of infrastructure; document bounded contexts and relationships when multiple models emerge. DDD is not permission to add ceremonial patterns or speculative abstractions.
- **Binding documentation:** this file is the arc42 architecture document and must retain all 12 numbered sections (1–12). Structural changes update the relevant sections in the same PR; significant decisions receive an ADR. The implementation roadmap is maintained separately and linked from this document.
- Third-party dependencies may contain unsafe internals, but require review of unsafe/FFI boundaries, advisories, license, maintenance, transitive dependencies, native libraries, and platform parity.
- UI thread is non-blocking; audio callbacks are real-time safe; queues are bounded.
- Monotonic timestamps drive media synchronization; validate external dimensions and use checked arithmetic.
- If a capability cannot be implemented through a suitable safe API, defer it or select another backend; never weaken the safety rule.

## Planning and work tracking

The authoritative project plan and progress tracking live in version-controlled Markdown files in this repository. GitHub Issues are not used for planning, backlog, bug, task, or phase tracking. Update the roadmap, specification, arc42 sections, and ADRs through reviewed Git changes as appropriate. Pull requests are used for code review and delivery only.

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

**Incremental delivery strategy:** implement user-visible capabilities as vertical slices. Connect the UI, core use case, platform/media/storage adapters, and desktop composition root as soon as each capability is ready; do not postpone integration to a final phase. Each slice must be exercisable in the running application, provide truthful availability/error states, and include a manual UI test procedure. Track implementation, automated testing, UI integration, and real-device validation as separate evidence states.

Domain-Driven Design complements the hexagonal architecture: the domain model and its invariants belong to the core; application use cases coordinate domain behavior and ports; adapters translate external concerns. The initial domain is the single Recording bounded context. Maintain a ubiquitous language and introduce additional contexts only when distinct models and ownership boundaries justify them.

1. Modular monolith: one desktop process and Rust workspace; no daemon or distributed architecture.
2. Hexagonal architecture: all inbound and outbound interactions cross application-owned ports; concrete adapters depend inward on core-owned contracts. This is mandatory for every production feature, not only OS capture.
3. Core-owned lifecycle: the Recording aggregate is authoritative for state transitions, cancellation, finalization outcome, and artifact publication.
4. Safe Rust: first-party crates forbid unsafe code; native/library unsafe boundaries remain isolated and reviewed.
5. Local-first: output is local, no telemetry or network dependencies.
6. Incremental validation: isolated PoCs prove platform/media behavior before any production adapter or dependency is approved.
7. Bounded concurrency: callbacks and UI remain non-blocking; media queues use explicit capacities and overload behavior.
8. Documentation as a gate: all 12 arc42 sections and ADRs track structural decisions; the roadmap defines phase exit criteria.

## 5. Building Block View

### 5.1 Level 1 — System

Framelet is a single desktop application. The core contains the Recording bounded context and application use cases. Inbound adapters accept user commands; outbound adapters implement core-owned ports for devices, media, clock, metadata, settings, and storage. The executable is the only composition root.

### 5.2 Level 2 — Modules

| Module | Responsibility | Depends on |
|---|---|---|
| `apps/desktop` | Slint UI, inbound command adapter, future composition root | Core; composition root may construct adapters |
| `crates/core` | Recording domain aggregate, recording settings value objects (`settings`), application use cases, ports, core-owned bounded data plane (`data_plane`), typed events/errors; with the `contract-tests` feature, shared port contract suites and fakes (`contract`) | capture-api contracts and standard library |
| `crates/capture-api` | Backend-neutral source descriptors and validated owned media blocks | Standard library |
| Future capture adapters | OS/device capture mechanics and native type translation | Core ports, capture-api |
| Future media adapter | Encoding, muxing and finalization; consumes the core-owned input streams it creates at prepare | Core media port and data plane, capture-api |
| `crates/storage` | Temp output reservation, publication of a closed output, abort, and recovery discovery | Core storage port; core `contract-tests` for its tests only |
| `crates/settings` | TOML settings file adapter (`TomlSettingsFile`): platform location, schema v1 mapping to core settings types, validated load with typed fallback to defaults, atomic save (ADR-0023) | Core settings port; `serde`, `toml` (the only crate allowed to use them); core `contract-tests` for its tests only |
| `poc/*` | Isolated, opt-in technology evaluations | Evaluation-only dependencies; never imported by core |

### 5.3 Hexagonal architecture and dependency rule (binding)

- **Core:** owns the domain model, application use cases, inbound ports, outbound ports, and application-level types/errors.
- **Inbound adapters:** UI and command handlers translate user intent into use-case calls.
- **Outbound adapters:** implement ports for capture devices, media runtime, filesystem, clock, and settings.
- **Composition root:** constructs adapters and injects them into core use cases. It is the only place allowed to know concrete adapter types.

Dependency direction is strictly inward: adapters may depend on core-owned contracts; core must not import adapter crates, Slint, OS APIs, or media-library types. Translate native/library types and errors at adapter boundaries. Never pass framework types through a port.

No UI-to-adapter shortcut, adapter-to-adapter dependency, global service locator, or concrete backend selection inside core is allowed. Core tests use fakes. Add compile-time dependency checks or architecture tests as the workspace grows.

~~~text
Inbound actor -> Inbound adapter -> Inbound port -> Application core
                                                |
                                     Outbound port (owned by core)
                                                |
                                  Outbound adapter -> OS/library/device
~~~

See ADR-0010. This rule is a release/PR acceptance criterion, not an aspirational target.

### 5.4 Domain model and DDD boundaries (binding)

DDD is the required modeling approach for Framelet's application domain. It works together with the hexagonal boundary: the domain model is inside the core; inbound and outbound adapters translate external concerns without owning business rules.

**Initial ubiquitous language** (to be refined as behavior is implemented):

| Term | Meaning / invariant |
|---|---|
| Recording | A user-initiated local capture session that produces one finalized recording artifact |
| Recording session | The lifecycle instance for prepare, start, pause, resume, stop, cancel, and finalize; state transitions are owned by the core |
| Capture source | A user-selected screen/window, camera, or microphone with explicit identity and capabilities |
| Media stream | Timestamped audio or video blocks belonging to a recording session and source |
| Recording mode | One of the explicitly supported combinations: screen, microphone-only, webcam, or screen + webcam; optional microphone where specified |
| Output artifact | A locally stored recording that is not considered successful until finalization and publication complete |
| Storage reservation | The exclusive temporary output storage creates for one recording: a storage session and the output target |
| Output target | Where the media pipeline writes a recording's output; issued by storage, forwarded unchanged by the core |
| Closed output | Proof returned by the media pipeline's close that the writer for one output target has drained and closed; the only thing storage publishes |
| Media sink / media stream | The producing (capture) and consuming (media) ends of one bounded, core-owned channel per source |
| Video drop | A stale queued frame discarded under overload; always counted |
| Audio discontinuity | Audio rejected under overload; counted and marked on the next accepted block, never silent |
| Source loss | A capture source disappearing during capture; its outcome (continue without the optional microphone, or stop) is a domain rule of the recording configuration |
| Degraded recording | A completed session that lost a source it could continue without (`lost_sources()` is not empty) |
| Recording settings | The persisted, user-editable preferences of the Recording context (explicit output directory, optional microphone on by default); defaults are safe and owned by the core |
| Output directory | A user-chosen recording directory: absolute, at most 4096 bytes, no control characters, no `..`; absent means the platform default Videos/Framelet directory |
| Settings origin | Where loaded settings came from: stored, defaults because nothing is stored, or defaults after a typed error that must be shown |

**Modeling rules:**
- Put domain concepts and invariants in `crates/core` (or a clearly separated domain module within the core); keep `capture-api` focused on transport/interoperability contracts rather than becoming a second domain model.
- Use entities for identity-bearing lifecycle concepts, value objects for validated immutable concepts (for example source identity, recording configuration, media time, and output destination), and aggregates only where a consistency boundary and invariants require them.
- The recording session is the initial aggregate: its root owns legal lifecycle transitions and invariants such as cancellation semantics and finalization outcome. Device handles and media buffers remain outside the aggregate.
- Application use cases coordinate domain objects and ports. Adapters own device/codec/filesystem mechanics and map native/persisted types to application-owned contracts.
- Domain events represent meaningful facts (for example recording started, source lost, audio gap detected, recording finalized); transport notifications are not automatically domain events.
- Do not introduce repositories, domain services, specifications, factories, or event buses unless a concrete invariant or use case needs them. Persistence abstractions belong at the application port boundary.
- Keep one ubiquitous language across code, tests, UI translations, ADRs, and arc42. Record changes to key terms and bounded-context boundaries here.

The initial MVP is one bounded context: **Recording**. Split additional bounded contexts only when independent models, invariants, or ownership justify it; do not pre-emptively create microservices or separate deployables.

### 5.5 Current state versus target

| Area | Status |
|---|---|
| Slint shell | Present; Windows native title bar is tinted to Framelet dark colors through the safe Winit Windows extension API; native decorations remain enabled |
| capture-api | Safe source descriptors, validated owned media blocks, and bounded worker-level audio queue implemented; production adapter wiring remains open |
| Wayland capture | Isolated XDG Portal/PipeWire PoC receives CPU-backed frames; real KDE/GNOME tests not run |
| Microphone capture | Isolated CPAL 0.18.2 PoC enumerates devices and receives default F32 samples into a bounded ring; real-device tests not run |
| core | Split into `domain` (Recording aggregate, source-requirement and source-loss invariants), `ports` (granular outbound contracts), `data_plane` (bounded sinks/streams with the video-drop and audio-discontinuity policy), `timeline`, and `application` (use cases). The application service orchestrates exclusively through the granular ports with typed `CapabilityFailure` errors and publishes lifecycle events (ADR-0017); capture adapters feed core-owned sinks created by the media pipeline at prepare, the output target flows from the storage reservation to the media pipeline, and publication requires the media pipeline's `ClosedOutput` (ADR-0020, ADR-0021). Shared contract suites and fakes live in `contract` behind the `contract-tests` feature. The settings port returns usable settings plus a typed origin and saves atomically (ADR-0023). Repository/metrics/clock use, and composing the settings port in the desktop app, remain open (roadmap A6, C2) |
| Media pipeline/compositing/encoding | Isolated GStreamer appsrc PoC sends synthetic owned audio/video through VP8/Vorbis and Matroska; bounded producer/worker queue, counted video drops, fatal audio overload, queue metrics, EOS, filesink-error and timeout tests exist; no production adapter, independent playback proof, sustained-load validation, or hardware benchmark evidence |
| Storage/recovery | `framelet-storage` adapter implements exclusive temp reservation, no-replace atomic publication where hard links are supported, abort, and non-destructive orphan discovery. All filesystem calls pass through a crate-private `FileOperations` seam used for adapter-level fault-injection tests (ADR-0014); real NTFS/ext4/btrfs fault runs remain open. Reservations carry the output target for the media pipeline; publication requires the matching `ClosedOutput`; abort is idempotent (ADR-0021). The adapter passes the shared storage contract suites (`crates/storage/tests/contract.rs`). Desktop composition validates the default output directory and exposes orphan scans; no production media writer or native directory picker yet |
| Settings | `framelet-settings` adapter stores `settings.toml` (schema v1) at the platform location, rejects malformed, unknown-version, and out-of-range files with typed errors while returning defaults and leaving the file untouched, and saves through temp file + rename with fault-injection tests at every step (ADR-0023, Proposed). It passes the shared settings contract suite (`crates/settings/tests/contract.rs`). Not yet constructed in `apps/desktop`; UI display of settings errors and use of the stored output directory are C2 slice work |
| Platform packaging | Not implemented |

## 6. Runtime View

The following sequences describe the target interaction across the granular ports. The use-case service already coordinates storage, media pipeline, capture, and event ports in the control-plane order fixed by ADR-0017 as amended by ADR-0020/ADR-0021, and is tested with fakes. Media payloads never pass through the service: they travel on core-owned bounded sinks from capture workers to the media pipeline (ADR-0020). The `framelet-storage` filesystem adapter exists behind `RecordingStoragePort` and is now used by the desktop composition root for output-directory validation and recovery scans. Concrete screen/camera/microphone/media adapters and native output-directory selection are not yet wired. Stop must stop producers, drain accepted media, finalize output, and only then publish a completed artifact. Adapter calls execute on workers, never on UI or real-time callback threads.

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
  Core->>Storage: begin (exclusive temporary output)
  Storage-->>Core: StorageReservation {session, OutputTarget}
  Core->>Media: prepare(id, configuration, OutputTarget)
  Media-->>Core: PreparedMedia {session, one bounded sink per source}
  User->>UI: Confirm start
  UI->>Core: Start
  Core->>Media: start
  Core->>Capture: start(source, sink) per source
  Capture-->>Media: Owned timestamped blocks via the sink (try_send)
  Media-->>Storage: Encoded/muxed data written to the OutputTarget
  Core-->>UI: Recording state and metrics
~~~

Exact picker invocation is adapter-dependent. Cancellation must not start capture or create a falsely completed output. If the prepared sinks do not match the configuration, the core aborts media and storage and reports a typed media-pipeline failure.

### 6.2 Stop and finalize

~~~mermaid
sequenceDiagram
  actor User
  participant Core
  participant Capture
  participant Media
  participant Storage
  User->>Core: Stop
  Core->>Capture: Stop all producers (sinks released)
  Capture-->>Core: Producers stopped
  Core->>Core: Check stream monitors for source loss
  Core->>Media: drain, then close
  Media-->>Storage: Final packets and container trailer
  Media-->>Core: ClosedOutput(target)
  Core->>Storage: publish(session, ClosedOutput)
  Storage-->>Core: Artifact (flush, link, no replace)
  Core-->>User: Completed(path)
~~~

Any required finalization failure produces a failure/recovery result, never success. Per ADR-0017 the core stops every capture source, then drains and closes the pipeline, and publishes only if all of those succeeded; every step is attempted even after an earlier failure, and on failure the temporary candidate is kept for recovery discovery instead of being aborted. A failure while starting rolls back already started sources, aborts the pipeline, and aborts the reservation. Storage rejects a `ClosedOutput` for any other target, and the core checks the target before publishing. Stop is idempotent: on a completed, failed, or cancelled session it returns that state and calls no port (REQ-LIFE-11). Storage `abort`, media `abort`, and capture `stop` are idempotent as well. Discard of an active recording is a separate explicit use case (REQ-LIFE-12, open).

### 6.3 Pause/resume and failures

Pause is coordinated across sources and pipeline. The core records monotonic pause boundaries and excludes paused duration from output timestamps. Exact semantics must be validated with the selected backend. Adapters emit typed permission, cancellation, unsupported-format, disconnect, and capture errors; the core decides whether to continue or stop. Adapters never silently substitute the selected source.

### 6.4 Source loss

A capture adapter that loses its device calls `report_source_lost` on its sink, which ends the stream as `SourceLost`. The inbound adapter's recording worker calls `process_source_losses` periodically (cadence fixed in Phase C). The core stops the lost capture, publishes `SourceLost { kind, source }`, records the loss on the aggregate, and applies `RecordingConfiguration::source_loss_outcome`: without the optional microphone the recording continues (degraded); losing a source the mode records stops the remaining sources, drains and closes the pipeline for recoverability, publishes nothing, keeps the candidate, and fails the session with `ApplicationError::SourceLost`. `stop` repeats the check after all producers have stopped, so a late loss still prevents publication (ADR-0020).

## 7. Deployment View

One installed desktop process; no daemon or helper service. The isolated media evaluation currently requires a separately installed GStreamer runtime and plugins; this native dependency is intentionally excluded from production application packages.

- Windows: executable plus explicitly packaged and reviewed runtime dependencies. The native title bar and caption buttons remain OS-owned; on Windows 11 build 22000+, the desktop shell sets title background, title text, and border colors through Winit. Older Windows versions retain the system title-bar appearance.
- Linux Wayland: executable plus documented portal/PipeWire integration and runtime dependencies.
- Linux X11: separate adapter/package validation; no implied Wayland parity.
- Output and settings stay in user-owned local directories. Settings: Linux `$XDG_CONFIG_HOME/framelet/settings.toml` (fallback `$HOME/.config/framelet/settings.toml`), directory `0700` and file `0600` when Framelet creates them; Windows `%APPDATA%\Framelet\settings.toml` under the per-user profile ACL (ADR-0023). No settings are written outside these paths.

The GStreamer media PoC uses appsrc, videoconvert, vp8enc, audioconvert, audioresample, vorbisenc, matroskamux, and filesink. This is a PoC-specific diagnostic plugin set, not an approved production allowlist. Linux CI exercises this set; Windows clean-machine deployment and a redistributable plugin/license inventory have not been validated. Native runtime/plugin inventory must be explicit and reproducible. Do not load arbitrary plugins from user-writable paths. Test packages on clean machines; recording must work offline.

## 8. Cross-cutting Concepts

### Ownership and concurrency

Use owned media blocks at asynchronous boundaries unless a safe lifetime-bound abstraction is demonstrably better. No borrowed view may outlive its source buffer. Worker cancellation and shutdown are explicit and observed.

### Backpressure

Every queue has a fixed capacity and policy. Video may drop stale frames to bound latency, but drops are counted and surfaced. Audio loss is never silent. Control events cannot be starved by media payloads.

Production contract (ADR-0020): every capture-to-media queue is a core-owned `data_plane` channel with a capacity fixed and validated at construction (ceilings 64 video frames, 1024 audio blocks); queue length can never exceed it. `try_send` never blocks: a full video queue discards its oldest frame and counts it (`VideoDelivery::DroppedStale`); a full audio queue rejects the newest block, keeps all accepted audio, counts the loss, and marks an `AudioGap` on the next accepted block (`AudioDelivery::Discontinuity`); loss after the last accepted block remains in the final `StreamStats::pending_gap`, which the media pipeline must account for after the stream ends. `StreamStats` exposes capacity, occupancy, high-water mark, accepted, delivered, dropped, lost audio frames, and discontinuity episodes. Control commands and source-loss flags do not share the payload path. Sinks are used by capture workers; real-time audio callbacks keep writing to a preallocated SPSC ring (ADR-0009). Surfacing the counters to metrics and the UI is Phase C (C-3).

The GStreamer evaluation PoC uses a fixed-capacity channel between its synthetic producer and pipeline worker. Producer sends are non-blocking: video uses counted drop-newest, while audio queue rejection fails the synthetic recording instead of silently introducing a gap. Queue capacity, high-water mark, accepted items, and rejected items are reported. The channel is not yet the final multi-source capture topology; sustained-load/control-path independence and real-device audio-gap policy remain validation gates. It does not promote the diagnostic PoC to a production media adapter.

### Time and synchronization

Use a monotonic clock and common origin. Preserve source timestamps where possible; track audio sample positions and video sequence numbers. Handle discontinuities, variable frame rates, pause segments, and drift. Wall-clock time may label files but never drives A/V sync. Test fixtures must make the expected audio/video timeline explicit so tests detect mismatched PTS, durations, and sample positions. The core `RecordingTimeline` aggregate maps monotonic source timestamps to PTS = active time since T0 minus half-open pause segments, reports samples before T0 or during pauses, per-stream regressions/duplicates and threshold-exceeding gaps explicitly, and uses checked arithmetic throughout; it is verified against a u128 reference oracle by exhaustive and seeded randomized tests (A-6).

### Security and privacy

Explicit user action and OS-mediated permissions; no network or telemetry. Never log captured media or unnecessary device identifiers. Validate dimensions, stride, formats, lengths, arithmetic, and paths. Restrict output permissions where supported. Review unsafe internals, FFI, advisories, licenses, native libraries, dynamic loading, and plugins.

### Settings persistence

Settings rules (value invariants, defaults, reject-not-clamp) live in `framelet_core::settings`; file format, location, and I/O live in `crates/settings` (ADR-0023). `load` is infallible: it returns usable settings and a `SettingsOrigin`, so a missing file is not an error while a malformed, unknown-version, oversized, unreadable, or invalid file yields defaults plus a typed `SettingsError` for the UI. A rejected file is never modified. `save` writes a same-directory temporary file, flushes it, and renames it over `settings.toml`; any failure returns `WriteFailed` and leaves the previous file byte-identical. A read-only settings file is respected. Settings I/O blocks and must run on a worker. serde/toml are confined to `crates/settings` by `scripts/check-architecture.py`; core types do not derive serde.

### Error handling

Typed errors cross module boundaries and become actionable UI messages. Preserve diagnostic causes without exposing sensitive content. Distinguish cancellation from denial and backend failure. No panic in production paths. Pipeline errors, missing terminal messages, and EOS timeout must be observable as failed finalization; candidate output is not reported as complete.

### Testing

Unit tests cover state/use cases, timestamps, settings, paths, checked buffer calculations. Contract tests cover adapter lifecycle, cancellation, disconnect, format negotiation, and queue bounds. The isolated media PoC tests its synthetic timeline, concurrent bounded queue behavior, pipeline-wide EOS, filesink failure, EOS timeout, and non-empty successful output. Encoder-specific error injection and interrupted-process recovery remain open. The filesystem adapter tests exclusive temp reservation, no-replace publication, abort, missing candidates, and non-destructive orphan discovery. The desktop shell reports the default output path and orphan count without deleting files. Native directory selection, platform-specific fault injection and crash durability remain open. Real-device integration tests cover every claimed OS/mode. Visual regression is separate from capture correctness. Long-run tests measure memory, audio continuity, sync, and finalization. The application service is tested with fake ports for call order, rollback, and failure reporting on every lifecycle path; requirement-to-test traceability is kept in [traceability.md](../verification/traceability.md). Shared port contract suites (`framelet_core::contract::{storage, media, capture, settings}`, `contract-tests` feature, ADR-0021, ADR-0023) run against the core fakes and against every real adapter; the filesystem storage adapter runs them in `crates/storage/tests/contract.rs` and the TOML settings adapter in `crates/settings/tests/contract.rs`. The settings adapter additionally injects a failure at every step of its atomic write and checks the previous file stays intact; its Unix permission tests skip with a message when run with privileges that bypass permissions. `scripts/check-architecture.py` confines the feature to `[dev-dependencies]`.

## 9. Architecture Decisions

See [ADR index](adr/README.md). The initial GStreamer evaluation is recorded in [ADR-0013](adr/ADR-0013-media-pipeline-evaluation.md), and the Phase 2 comparison/continued-deferral decision is recorded in [ADR-0015](adr/ADR-0015-media-backend-evaluation-decision.md). The filesystem storage adapter slice is recorded in [ADR-0014](adr/ADR-0014-transactional-filesystem-storage.md); ADR-0015 preserves the deferred production media choice in ADR-0004.

| ADR | Decision | Status |
|---|---|---|
| ADR-0001 | Modular monolith and ports-and-adapters | Accepted |
| ADR-0002 | Core-owned recording lifecycle | Accepted |
| ADR-0003 | Safe Rust and platform adapters | Accepted |
| ADR-0004 | Defer media runtime selection until PoC | Accepted (defer) |
| ADR-0005 | Owned media blocks and explicit clock contracts | Accepted for PoC |
| ADR-0006 | Start Wayland portal PoC with lamco-wayland | Accepted for evaluation only |
| ADR-0007 | Lower-level PipeWire thread manager for the isolated frame PoC | Accepted for PoC |
| ADR-0008 | Evaluate CPAL for microphone capture | Evaluation only |
| ADR-0009 | Bounded worker-level audio block transport | Accepted for backend-neutral API |
| ADR-0010 | Make hexagonal architecture and arc42 compliance binding | Accepted |
| ADR-0011 | Make Domain-Driven Design binding for the application domain | Accepted |
| ADR-0012 | Define granular Recording bounded-context port boundaries | Accepted; decision 10 superseded by ADR-0017; decisions 3/4 amended by ADR-0020/ADR-0021 |
| ADR-0013 | Evaluate GStreamer through an isolated media pipeline PoC | Proposed — evaluation only |
| ADR-0014 | Transactional filesystem recording storage | Accepted for the Phase 3 adapter slice; hand-off and abort semantics amended by ADR-0021 |
| ADR-0015 | Complete media pipeline evaluation; keep production backend deferred | Accepted — evaluation decision only; selection gates amended by ADR-0018 |
| ADR-0016 | Native dark Windows title bar | Accepted for the Windows desktop shell |
| ADR-0017 | Orchestrate recording use cases through the granular ports | Accepted; supersedes ADR-0012 decision 10; call order amended by ADR-0020/ADR-0021 |
| ADR-0018 | Risk-first re-sequencing and two-stage media backend decision | Accepted — planning/decision process |
| ADR-0019 | Slint-generated entry points use `deny(unsafe_code)` | Accepted — records existing exception |
| ADR-0020 | Core-owned bounded data plane, overload policy, and source loss | Accepted (2026-11-28) |
| ADR-0021 | Storage-to-media hand-off through the core, and shared port contract suites | Accepted (2026-11-28) |
| ADR-0022 | Mutation testing of `framelet-core` with cargo-mutants | Accepted (2026-11-28) |
| ADR-0023 | Persist recording settings as a TOML file in a dedicated settings adapter | Proposed |

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
| Q-10 | Resource finalization fails | Session becomes Failed and never exposes a completed artifact | Fake-port application test |
| Q-11 | Illegal lifecycle command | Core rejects the command without invoking the external resource port | Aggregate/application unit test |
| Q-12 | Media consumer stops accepting buffers | Queue stays within capacity; overload is counted; audio loss is explicit | Isolated media PoC stress test |
| Q-13 | One stream ends before the other | Finalization waits for both branches or reports timeout/error; no false completion | Isolated media PoC test |
| Q-14 | Pipeline fails before EOS | Candidate output is removed or explicitly marked incomplete; no completed artifact is printed | Isolated media PoC failure test |

SPECIFICATION.md contains performance targets. Publish measurements only with hardware, OS, backend, resolution, frame rate, codecs, duration, and encoder mode.

## 11. Risks and Technical Debt

| Risk | Impact | Mitigation / evidence required |
|---|---|---|
| Safe wrapper lacks required OS behavior | Platform capability blocked under no-unsafe policy | Test public safe APIs early; defer rather than add unsafe glue |
| GStreamer runtime/plugin footprint | Package complexity and support burden | Minimal plugin inventory and clean-machine PoC |
| Dual-stream timestamp and EOS behavior | A/V drift or incomplete containers | Independent playback, synthetic clock tests, injected errors and stop-to-EOS measurements in the isolated media PoC |
| Wayland desktop differences | Uneven Linux behavior | GNOME and KDE tests; portal session/cancel handling |
| Audio overload | Gaps and sync errors | Bounded ring buffer, counters, stress/device tests |
| Pause semantics differ | Timestamp discontinuities | Common timeline contract and conformance tests |
| MP4 crash recovery limited | Partial file may be unusable | Temp output, documented recovery, evaluate fragmented formats |
| Hard-link publication unavailable | Filesystem may not support same-volume hard links | Fail closed; validate target filesystems and evaluate safe no-replace alternative only through a new ADR |
| Device formats vary | Unsupported mode or expensive conversion | Explicit negotiation and capability reporting |
| Plugin discovery risk | Supply-chain/runtime loading risk | Explicit plugin allowlist and packaged paths |
| Capture API too small for streaming | Expensive API rework | Data-plane contract implemented in Phase A (ADR-0020, accepted 2026-11-28) before any production adapter |
| Sink lock contention or misuse from an audio callback | Glitches or priority inversion | `try_send` is documented as worker-only; callbacks keep the SPSC ring (ADR-0009); measure contention under D-4 load |
| Source-loss polling latency | Late reaction to an unplugged device | Phase C fixes and measures the cadence (target ≤ 100 ms); revisit push notification if needed (ADR-0020) |
| Continuing without a lost optional microphone produces a degraded recording | User may not notice missing audio | Loss is published and recorded on the aggregate; degraded completion must be shown in the UI (Phase C, REQ-ERR-06) |
| No safe Windows Graphics Capture wrapper | Windows screen capture blocked by the no-unsafe rule | Roadmap spike B1 with Go/No-Go ADR before Phase E2 |
| H.264/AAC encoders not redistributable under acceptable terms | Default MP4/H.264/AAC output format must change | Roadmap spike B3 encoder/licence matrix before the provisional backend decision |
| Webcam backend immature on one OS | Webcam modes unavailable on that OS | Roadmap spike B2 with Go/No-Go ADR |
| One invalid settings value resets all settings to defaults | User loses preferences until the file is fixed | File is preserved, error names the line or rule; whole-file rejection is a deliberate ADR-0023 decision awaiting confirmation |
| `APPDATA`/`HOME` missing or redirected | Settings location unavailable | Typed `LocationUnavailable` with defaults; revisit a Known Folder lookup if observed (ADR-0023 alternatives) |

Implementation sequencing and phase exit criteria are defined in [Implementation roadmap](implementation-roadmap.md); how each criterion is proven is defined in the [verification framework](../verification/README.md). Open questions: safe Windows capture wrapper; safe Wayland portal/PipeWire API; CPAL sample-format coverage, device-loss behavior and cross-platform quality; nokhwa cross-platform quality; GStreamer versus alternatives; owned pixel/audio formats; measured queue capacities.

## 12. Glossary

| Term | Meaning |
|---|---|
| Adapter | Platform/library-specific implementation of an application-owned port |
| Aggregate | Cluster of domain objects whose invariants are enforced through one root |
| Bounded context | Explicit boundary within which a domain model and its language are consistent |
| Domain event | A fact of domain significance that has occurred |
| Entity | Domain object defined by identity and lifecycle |
| Ubiquitous language | Shared, precise vocabulary used by domain experts and the delivery team |
| Value object | Immutable domain concept defined by its values |
| Backpressure | Preventing a fast producer from overwhelming a slower consumer |
| Capture source | User-selected display, window, camera, or microphone |
| Core | Application layer owning use cases and recording lifecycle |
| Muxing | Combining encoded streams into a media container |
| Monotonic clock | Clock suitable for elapsed time that does not move backwards |
| T0 (timeline origin) | Monotonic instant chosen by core at start; presentation time 0 |
| Pause segment | Half-open monotonic interval `[start, end)` excluded from presentation time |
| PTS (presentation timestamp) | Active nanoseconds since T0 with all pause segments removed |
| Discontinuity | Pause-excluded gap between consecutive samples of a stream that exceeds the configured threshold |
| Port | Application-owned interface for an external capability |
| Contract suite | Adapter-agnostic test functions that fakes and real adapters of one port must both pass |
| Closed output | Media pipeline's proof that the writer for an output target has closed; required to publish |
| PipeWire | Linux multimedia transport for portal-provided streams |
| XDG Desktop Portal | Desktop-mediated interface for user-controlled resource access |
