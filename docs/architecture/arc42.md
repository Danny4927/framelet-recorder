# Framelet — arc42 Software Architecture

**Version:** 0.8 — Phase 2 media evaluation validation in progress  
**Date:** 17 November 2026  
**Status:** Proposed architecture; capture and media technology choices remain subject to PoC gates.

> This document describes the intended MVP architecture. The repository currently contains a Slint shell, safe capture data contracts, an isolated Wayland frame PoC, an isolated CPAL microphone PoC, a bounded worker-level audio queue, the initial Recording session aggregate and use cases, core-owned source/media/storage/clock/settings/repository/event/metrics ports, and an isolated GStreamer media evaluation PoC. No production media backend has been selected. Native capture has not been integrated into the production application; media processing, storage finalization, and real-device validation remain open.

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
| `crates/core` | Recording domain aggregate, application use cases, ports, typed events/errors | capture-api contracts and standard library |
| `crates/capture-api` | Backend-neutral source descriptors and validated owned media blocks | Standard library |
| Future capture adapters | OS/device capture mechanics and native type translation | Core ports, capture-api |
| Future media adapter | Encoding, muxing, bounded media queues and finalization | Core media port, capture-api |
| Future storage adapter | Temp output, flush/close, publication and recovery | Core storage port |
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
| Slint shell | Present; scaffold only |
| capture-api | Safe source descriptors, validated owned media blocks, and bounded worker-level audio queue implemented; production adapter wiring remains open |
| Wayland capture | Isolated XDG Portal/PipeWire PoC receives CPU-backed frames; real KDE/GNOME tests not run |
| Microphone capture | Isolated CPAL 0.18.2 PoC enumerates devices and receives default F32 samples into a bounded ring; real-device tests not run |
| core | Recording aggregate, inbound use-case port, granular outbound ports, typed errors/events, metrics snapshots and fakeable orchestration; concrete adapters and durable media storage remain open |
| Media pipeline/compositing/encoding | Isolated GStreamer appsrc PoC sends synthetic owned audio/video through VP8/Vorbis and Matroska EOS finalization; no production adapter, independent playback proof, overload validation, or benchmark evidence yet |
| Storage/recovery | Not implemented |
| Platform packaging | Not implemented |

## 6. Runtime View

The following sequences describe the target interaction across the granular ports. In the Phase 1 implementation, the use-case service is exercised with a fakeable `RecordingResources` facade; concrete screen/camera/microphone/media/storage adapters and the desktop composition root are deliberately not wired yet. Stop must stop producers, drain accepted media, finalize output, and only then publish a completed artifact. Adapter calls execute on workers, never on UI or real-time callback threads.

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

One installed desktop process; no daemon or helper service. The isolated media evaluation currently requires a separately installed GStreamer runtime and plugins; this native dependency is intentionally excluded from production application packages.

- Windows: executable plus explicitly packaged and reviewed runtime dependencies.
- Linux Wayland: executable plus documented portal/PipeWire integration and runtime dependencies.
- Linux X11: separate adapter/package validation; no implied Wayland parity.
- Output and settings stay in user-owned local directories.

The GStreamer media PoC uses appsrc, videoconvert, vp8enc, audioconvert, audioresample, vorbisenc, matroskamux, and filesink. This is a PoC-specific diagnostic plugin set, not an approved production allowlist. Native runtime/plugin inventory must be explicit and reproducible. Do not load arbitrary plugins from user-writable paths. Test packages on clean machines; recording must work offline.

## 8. Cross-cutting Concepts

### Ownership and concurrency

Use owned media blocks at asynchronous boundaries unless a safe lifetime-bound abstraction is demonstrably better. No borrowed view may outlive its source buffer. Worker cancellation and shutdown are explicit and observed.

### Backpressure

Every queue has a fixed capacity and policy. Video may drop stale frames to bound latency, but drops are counted and surfaced. Audio loss is never silent. Control events cannot be starved by media payloads. Queue capacities remain open until formats and throughput are measured. The Phase 2 validation slice adds regression coverage for the chosen queue-full policy and timestamp continuity; it does not promote the diagnostic PoC to a production media adapter.

### Time and synchronization

Use a monotonic clock and common origin. Preserve source timestamps where possible; track audio sample positions and video sequence numbers. Handle discontinuities, variable frame rates, pause segments, and drift. Wall-clock time may label files but never drives A/V sync. Test fixtures must make the expected audio/video timeline explicit so tests detect mismatched PTS, durations, and sample positions.

### Security and privacy

Explicit user action and OS-mediated permissions; no network or telemetry. Never log captured media or unnecessary device identifiers. Validate dimensions, stride, formats, lengths, arithmetic, and paths. Restrict output permissions where supported. Review unsafe internals, FFI, advisories, licenses, native libraries, dynamic loading, and plugins.

### Error handling

Typed errors cross module boundaries and become actionable UI messages. Preserve diagnostic causes without exposing sensitive content. Distinguish cancellation from denial and backend failure. No panic in production paths. Pipeline errors, missing terminal messages, and EOS timeout must be observable as failed finalization; candidate output is not reported as complete.

### Testing

Unit tests cover state/use cases, timestamps, settings, paths, checked buffer calculations. Contract tests cover adapter lifecycle, cancellation, disconnect, format negotiation, and queue bounds. The isolated media PoC tests its synthetic timeline, queue overload behavior, terminal EOS/error handling, and incomplete-output cleanup. Real-device integration tests cover every claimed OS/mode. Visual regression is separate from capture correctness. Long-run tests measure memory, audio continuity, sync, and finalization.

## 9. Architecture Decisions

See [ADR index](adr/README.md). The initial GStreamer evaluation is recorded in [ADR-0013](adr/ADR-0013-media-pipeline-evaluation.md); the next validation slice is implementation work only and does not supersede the deferred production choice in ADR-0004.

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
| ADR-0012 | Define granular Recording bounded-context port boundaries | Accepted |
| ADR-0013 | Evaluate GStreamer through an isolated media pipeline PoC | Proposed — evaluation only |

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
| Device formats vary | Unsupported mode or expensive conversion | Explicit negotiation and capability reporting |
| Plugin discovery risk | Supply-chain/runtime loading risk | Explicit plugin allowlist and packaged paths |
| Capture API too small for streaming | Expensive API rework | Design ownership and event contract during PoC |

Implementation sequencing and phase exit criteria are defined in [Implementation roadmap](implementation-roadmap.md). Open questions: safe Windows capture wrapper; safe Wayland portal/PipeWire API; CPAL sample-format coverage, device-loss behavior and cross-platform quality; nokhwa cross-platform quality; GStreamer versus alternatives; owned pixel/audio formats; measured queue capacities.

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
| Port | Application-owned interface for an external capability |
| PipeWire | Linux multimedia transport for portal-provided streams |
| XDG Desktop Portal | Desktop-mediated interface for user-controlled resource access |
