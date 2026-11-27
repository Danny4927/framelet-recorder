# Framelet — Product & Technical Specification

**Status:** MVP specification  
**Product:** Framelet  
**Repository:** `Danny4927/framelet-recorder`  
**Language:** Rust  
**Platforms:** Windows 10/11 and Linux (Wayland; X11 subject to backend validation)  
**macOS:** Explicitly unsupported

---

## 1. Product definition

Framelet is a small, performant, local-first desktop recorder. It records screen, microphone, webcam, or screen plus webcam to a local file. The application must be simple enough that a user can start a recording with only a few deliberate interactions. [REQ-UX-01]

### 1.1 MVP scope

The MVP includes exactly these four modes:

- **Screen:** capture a user-selected display, window, or source supported by the OS/backend. Microphone audio is optional. [REQ-MODE-SCREEN] [REQ-LIFE-09]
- **Microphone:** record the selected microphone to a local audio file. [REQ-MODE-MIC] [REQ-LIFE-08]
- **Webcam:** record the selected camera. Microphone audio is optional. [REQ-MODE-CAM] [REQ-LIFE-09]
- **Screen + webcam:** composite the camera into the screen video as a configurable picture-in-picture bubble. Microphone audio is optional. [REQ-MODE-PIP] [REQ-LIFE-09]

System audio is not part of the MVP. [REQ-SCOPE-01]

### 1.2 Explicitly out of scope

No cloud, network, upload, account, authentication, workspace, library, collaboration, comments, reactions, analytics, streaming, browser extension, mobile app, AI, transcription, subtitles, video editing, stitching, or in-app video player. [REQ-SCOPE-02]

The application must not require an internet connection to record. [REQ-SEC-01]

### 1.3 Product principles

1. Local-first and offline-capable. [REQ-SEC-01]
2. Privacy by default; capture starts only after explicit user action. [REQ-LIFE-13]
3. Minimal UI and low resource consumption. [REQ-UX-02] [REQ-PERF-01]
4. Rust for first-party application logic. [REQ-SAFE-03]
5. No first-party `unsafe` Rust code. [REQ-SAFE-01]
6. No bypassing ownership, borrowing, type, or OS permission mechanisms. [REQ-SAFE-04]
7. Bounded memory and explicit backpressure. [REQ-FLOW-01]
8. Errors are visible; recordings must never be silently reported as successful if data was lost or finalization failed. [REQ-LIFE-04] [REQ-LIFE-07]

---

## 2. Platform support

### 2.1 Windows

Target Windows 10/11. Prefer Windows Graphics Capture for user-mediated display/window capture. Evaluate the supported Windows API and crate maturity during the capture PoC. Respect Windows camera and microphone privacy controls. [REQ-PLAT-WIN] [REQ-SAFE-04]

### 2.2 Linux

Wayland is a first-class target and must use XDG Desktop Portal for user-mediated source selection/permission and PipeWire for capture streams. Do not bypass compositor or portal restrictions. [REQ-PLAT-WAYLAND]

X11 may use a separate backend. X11 support must be documented separately because its security and capture model differs from Wayland. [REQ-PLAT-X11]

### 2.3 macOS

Not supported. Do not add macOS-specific code, dependencies, CI targets, packaging, or documentation promising support. [REQ-PLAT-MACOS]

### 2.4 Platform abstraction

Keep OS-specific capture behind internal Rust traits. The application core must not depend directly on a specific capture crate or native API. [REQ-SAFE-02]

---

## 3. Recommended technology direction

These are candidates, not unconditional dependency approvals. Pin exact versions only after a current review of crates.io, upstream release notes, security advisories, license, maintenance, native dependencies, and unsafe implementation. [REQ-SEC-02]

| Area | Candidate | Notes |
|---|---|---|
| Language | Rust stable, Edition 2024 | Pin toolchain in repository |
| GUI | Slint | Native declarative UI; validate packaging and resource use |
| Screen capture | pinray PoC; compare native Windows Graphics Capture and Linux portal/PipeWire adapters | pinray is treated as API-unstable until proven otherwise |
| Microphone | CPAL | Real-time callback must remain bounded and non-blocking |
| Webcam | nokhwa PoC; compare platform-native backends | Validate device enumeration and formats on Windows/Linux |
| Media pipeline | GStreamer Rust bindings | Native runtime/plugins affect packaging and footprint |
| Logging | tracing | Never log captured content or sensitive device data |
| Config | serde + TOML | Validate values and handle malformed config safely |
| Testing | cargo test, Clippy, property/fuzz tests | No unsafe test shortcuts |

### 3.1 Technology selection gates

Before adopting a dependency, document: [REQ-SEC-02]
- Current release and maintenance activity.
- License and redistribution requirements.
- Known security advisories.
- Direct and transitive dependencies.
- Native libraries and plugins required.
- Whether public APIs are safe Rust APIs.
- Unsafe code in the dependency and its FFI boundary.
- Windows and Linux feature parity.
- Packaging and runtime footprint.
- Failure behavior, cancellation, and device disconnect behavior.

A dependency is not approved merely because it is popular or convenient. [REQ-SEC-02]

---

## 4. User experience

### 4.1 Main window

The main window should be compact and uncluttered. It contains: [REQ-UX-03]
- Four large mode choices.
- Microphone selector and enable/disable control.
- Camera selector when relevant.
- Camera preview when relevant.
- Recording destination.
- A prominent Start Recording button.
- Minimal settings for output quality and webcam bubble.

Do not expose advanced codec or pipeline options in the primary view. [REQ-UX-04]

### 4.2 Recording flow

1. User chooses a mode. [REQ-UX-05]
2. Framelet enumerates available devices and supported formats. [REQ-DEV-01]
3. User selects the capture source through the OS-provided picker where applicable. [REQ-CAP-01]
4. Required permissions are requested by the OS. [REQ-SAFE-04]
5. Preview is shown for camera modes. [REQ-MODE-CAM]
6. User explicitly starts recording. [REQ-LIFE-13]
7. Optional three-second countdown. [REQ-UX-06]
8. Recording status, elapsed time, Pause, Resume, and Stop are shown. [REQ-UX-07]
9. On stop, sources are closed, buffers drained, and the output container finalized. [REQ-LIFE-14]
10. The UI shows the resulting file path only after successful finalization. [REQ-LIFE-03]

The UI must remain responsive while recording. The application must not record before the user presses Start. [REQ-UX-08] [REQ-LIFE-13]

### 4.3 Recording controls

- Start [REQ-UX-07]
- Pause [REQ-UX-07]
- Resume [REQ-UX-07]
- Stop [REQ-UX-07]
- Cancel/discard [REQ-LIFE-12]
- Camera preview [REQ-MODE-CAM]

Stop must be idempotent. Cancel must clearly warn that the current recording will be discarded. [REQ-LIFE-11] [REQ-LIFE-12]

### 4.4 Webcam bubble

For Screen + webcam:
- Default position: bottom-right. [REQ-PIP-01]
- Default size: approximately 20% of output width. [REQ-PIP-02]
- Default source aspect ratio preserved; crop/fit behavior is consistent. [REQ-PIP-03]
- Configurable size: small, medium, large. [REQ-PIP-04]
- Configurable corner: top-left, top-right, bottom-left, bottom-right. [REQ-PIP-05]
- Optional preview mirroring must not mirror the screen. [REQ-PIP-06]
- Bubble is composited into the output video. [REQ-MODE-PIP]
- Dynamic repositioning during recording is not required for MVP. *(informative)*

### 4.5 Suggested defaults

These defaults apply where the selected device and backend support them: [REQ-UX-09]

| Setting | Default |
|---|---|
| Screen output | Up to 1920×1080 |
| Frame rate | 30 FPS |
| Webcam | Up to 1280×720 |
| Audio sample rate | 48 kHz when supported |
| Countdown | 3 seconds |
| Container | MP4, subject to platform pipeline validation |
| Video codec | H.264 |
| Audio codec | AAC |
| Screen + webcam bubble | Bottom-right |

Device-supported formats take precedence. The app must never claim a setting is active if the selected device/backend cannot provide it. [REQ-DEV-02]

---

### 4.6 Incremental product delivery

Features must become testable through the running desktop application as soon as a vertical slice is complete. Do not defer all UI wiring to the end of the project. A UI control must invoke a real implemented use case or be visibly unavailable with an explanation; decorative controls and simulated success states are not acceptable. For every capability, distinguish implementation, automated test coverage, UI integration, and real-device validation. Each increment must include a short manual test procedure and preserve already-working UI flows. [REQ-LIFE-10] [REQ-PROC-01]

### 4.7 Planning and work tracking

Maintain project planning exclusively as Markdown in the Git repository. Do not create GitHub Issues for tasks, bugs, backlog items, or phase tracking. The roadmap and architecture/specification documents are the source of truth; update them through normal reviewed Git changes. Pull requests are used for reviewing and delivering implementation changes. [REQ-PROC-02]

## 5. Architecture

Use a modular Rust workspace and a Rust-first application core. No backend or services. [REQ-ARCH-01]

Suggested layout:

```text
framelet-recorder/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── deny.toml
├── README.md
├── SPECIFICATION.md
├── SECURITY.md
├── AGENTS.md
├── apps/
│   └── desktop/
└── crates/
    ├── core/
    ├── capture-api/
    ├── capture-screen/
    ├── capture-camera/
    ├── capture-audio/
    ├── media-pipeline/
    ├── storage/
    ├── config/
    └── platform/
```

Create workspace members only when they contain valid manifests and buildable code; do not commit a knowingly broken placeholder workspace. [REQ-ARCH-02]

### 5.1 Module responsibilities

Each module has the following responsibility: [REQ-ARCH-05]

| Module | Responsibility |
|---|---|
| desktop | Application entry point and Slint UI |
| core | Use cases, recording state machine, orchestration |
| capture-api | Safe shared interfaces and owned frame/audio types |
| capture-screen | OS-specific screen capture adapters |
| capture-camera | Camera enumeration and frame capture |
| capture-audio | Microphone enumeration and audio capture |
| media-pipeline | Timestamping, compositing, encoding, muxing |
| storage | Safe local file creation/finalization |
| config | Validated local settings |
| platform | OS-specific integration and capability detection |

UI code must not perform capture, encoding, disk I/O, or blocking device enumeration. [REQ-ARCH-03]

### 5.2 Capture interfaces

Define narrow, safe traits for capture lifecycle and events. Prefer event streams/channels over blocking UI calls. All frames must have clear ownership; no borrowed pointer may outlive the source buffer. [REQ-CAP-02]

Frame metadata must include: [REQ-CAP-03]
- Width and height.
- Stride/row pitch.
- Pixel format.
- Monotonic timestamp.
- Sequence number.
- Owned pixel data or a safe, lifetime-bound buffer abstraction.

Audio blocks must include: [REQ-CAP-04]
- Sample rate.
- Channel count.
- Sample format.
- Monotonic timestamp/sample position.
- Owned or safely borrowed sample storage with explicit lifetime.

Validate all dimensions, strides, lengths, and arithmetic before allocating or processing buffers. Use checked arithmetic. [REQ-CAP-05]

### 5.3 State machine

States:

```text
Idle -> Preparing -> Ready -> Recording <-> Paused
                                      |
                                      v
                                  Finalizing
                                  /        \
                           Completed      Failed
```

Additional typed errors include: [REQ-ERR-01]
- PermissionDenied
- SourceSelectionCancelled
- DeviceUnavailable
- CaptureFailed
- EncoderFailed
- StorageFailed
- InsufficientDiskSpace
- UnsupportedFormat
- PipelineOverrun

Only the core owns the state machine. UI reflects state events and cannot mutate state independently. [REQ-LIFE-01] [REQ-LIFE-02]

### 5.4 Threading and backpressure

- UI event loop remains non-blocking. [REQ-UX-08]
- Screen capture, camera capture, media processing, and storage run outside the UI thread. [REQ-ARCH-04]
- Audio callbacks must be real-time safe: no blocking, filesystem/network I/O, mutex waits, or expensive allocations. [REQ-FLOW-04]
- Transfer audio using a bounded single-producer/single-consumer ring buffer where suitable. [REQ-FLOW-05]
- Use bounded channels for video and control events. [REQ-FLOW-01]
- Never allow queues to grow without limit. [REQ-FLOW-01]
- Video may drop stale frames under overload, but drops must be counted and surfaced. [REQ-FLOW-02]
- Audio loss must be detected and reported; never silently conceal it. [REQ-FLOW-03]
- Shutdown and cancellation must propagate to every worker and release all capture resources. [REQ-LIFE-06]

---

## 6. Media pipeline

### 6.1 Modes

- Screen: screen video plus optional microphone. [REQ-MODE-SCREEN] [REQ-LIFE-09]
- Microphone: audio-only output. [REQ-MODE-MIC]
- Webcam: camera video plus optional microphone. [REQ-MODE-CAM] [REQ-LIFE-09]
- Screen + webcam: screen and camera composed into one video plus optional microphone. [REQ-MODE-PIP] [REQ-LIFE-09]

### 6.2 Synchronization

Use a monotonic clock. At recording start define a common origin T0. Map source timestamps onto a shared presentation timeline. Do not use wall-clock time as the synchronization source. [REQ-TIME-01]

Requirements:
- Preserve source timestamps where possible. [REQ-TIME-06]
- Handle variable frame rate and dropped frames. [REQ-TIME-07]
- Keep audio sample position and timestamps consistent. [REQ-TIME-08]
- Remove paused duration from the final timeline. [REQ-TIME-02]
- Detect timestamp discontinuities and clock drift. [REQ-TIME-03] [REQ-TIME-09]
- Test A/V sync over long recordings. [REQ-TIME-05]

### 6.3 Encoding and containers

Preferred output:
- Video: MP4/H.264. [REQ-MEDIA-01]
- Audio: AAC in MP4/M4A. [REQ-MEDIA-02]
- Microphone-only: M4A/AAC. [REQ-MODE-MIC] [REQ-MEDIA-02]

Hardware encoding may be preferred when it is available, supported, and stable. A software encoder fallback is required. Never assume a hardware encoder exists. [REQ-MEDIA-03]

The GStreamer pipeline and required plugins must be tested and packaged explicitly for each supported OS. Do not dynamically load arbitrary plugins from user-writable directories. A completed file must not be reported until the container is finalized successfully. [REQ-MEDIA-04] [REQ-SEC-03] [REQ-LIFE-03]

### 6.4 Compositing

For Screen + webcam, composite the camera into the screen frame. Preserve aspect ratio, use a deterministic fit/crop policy, and avoid unnecessary frame copies. GPU compositing may be used only through safe, reviewed APIs and only if it does not compromise portability or reliability. [REQ-PIP-03] [REQ-PIP-07]

---

## 7. Local storage

Default output directory:
- Windows: user's Videos/Framelet directory. [REQ-STOR-06]
- Linux: user's Videos/Framelet directory, resolved using standard user-directory conventions. [REQ-STOR-06]

Requirements:
- User can select another directory. [REQ-STOR-04]
- Never overwrite an existing recording. [REQ-STOR-01]
- Use collision-resistant generated filenames. [REQ-STOR-07]
- Create temporary output files exclusively. [REQ-STOR-02]
- Finalize and rename only after successful muxer/container close. [REQ-STOR-02]
- Validate paths and handle symlinks carefully. [REQ-STOR-08]
- Do not invoke a shell to create files or directories. [REQ-STOR-09]
- Do not accept arbitrary path fragments from untrusted metadata. [REQ-STOR-10]
- Restrict file permissions to the current user where supported. [REQ-STOR-11]
- Detect low disk space and write failures. [REQ-STOR-05]
- On startup, identify incomplete temporary recordings and offer safe recovery/discard where possible. [REQ-STOR-03]

A crash or power failure cannot be guaranteed to preserve a valid MP4. Recovery behavior must be documented and tested. [REQ-STOR-12]

---

## 8. Performance goals

These are targets to measure, not guarantees: [REQ-PERF-01]

| Metric | Target |
|---|---|
| Idle RAM | <100 MB |
| 1080p recording RAM | Aim <300 MB |
| Output | Up to 1080p30 |
| UI responsiveness | <100 ms for ordinary interactions |
| Start latency | <2 seconds after source/permission selection, excluding OS picker delay |
| A/V sync | <100 ms under supported configurations [REQ-TIME-04] |
| Memory growth | Bounded during long recordings |
| CPU | Hardware encoding preferred when available |

Measure full process RSS, including native libraries and plugins. Publish benchmark hardware, OS, codec, and settings. If a target cannot be met, document the result rather than hiding it. [REQ-PERF-01]

---

## 9. Error handling

Errors must be typed, actionable, and visible. No silent failure. [REQ-LIFE-07] [REQ-ERR-02]

Examples:
- Permission denied: explain which OS permission is required and where to enable it. [REQ-ERR-03]
- Source selection cancelled: return to Ready without starting. [REQ-ERR-04]
- Camera disconnected: stop safely or continue only if the active mode remains valid and the user is informed. [REQ-ERR-05]
- Microphone disconnected: report the loss; do not silently produce a falsely complete recording. [REQ-ERR-06]
- Disk full: stop safely, attempt finalization, preserve recoverable data. [REQ-LIFE-05] [REQ-STOR-05]
- Encoder failure: stop pipeline and report whether output is recoverable. [REQ-ERR-07]
- Queue overrun: count drops and surface degraded recording quality. [REQ-FLOW-02]

Avoid panics in production paths. Do not use `unwrap()` or `expect()` on fallible runtime operations. [REQ-SAFE-06]

---

## 10. Testing and acceptance

### 10.1 Unit tests

Test state transitions, path validation, filename collisions, settings validation, timestamp mapping, checked buffer calculations, queue limits, and error mapping. [REQ-TEST-01]

### 10.2 Integration tests

On every supported OS: [REQ-TEST-02]
- Screen recording.
- Microphone-only recording.
- Webcam recording.
- Screen + webcam.
- Pause/resume.
- Stop/cancel.
- Permission denied/cancelled.
- Device unplugged during recording.
- Unsupported device format.
- Disk full/write failure.
- Long recording and A/V sync.
- App shutdown during recording.

### 10.3 Acceptance criteria

MVP is complete only when all four modes produce valid local files, the app works offline, permissions are respected, no existing file is overwritten, UI remains responsive, memory remains bounded, errors are visible, and security/CI checks pass. [REQ-ACC-01]

---

## 11. Implementation sequence

The binding implementation order, phase dependencies, exit criteria, and their proofs are defined in [docs/architecture/implementation-roadmap.md](docs/architecture/implementation-roadmap.md); how completeness and correctness are proven is defined in [docs/verification/README.md](docs/verification/README.md). This specification does not define a separate sequence.

The guiding principles of that order are: retire the largest platform and licensing risks first with time-boxed spikes; deliver a real, recordable walking skeleton early instead of a polished UI on top of unproven capture; integrate every finished capability into the running app immediately; validate Wayland and X11 separately; and treat the security, performance, and packaging reviews as gates that start early and close before release.

---

## 12. Definition of Done

- [ ] Windows 10/11 supported and tested.
- [ ] Linux Wayland supported and tested.
- [ ] X11 support explicitly tested or explicitly marked unsupported.
- [ ] macOS is not supported.
- [ ] All four recording modes work.
- [ ] Optional microphone works in applicable modes.
- [ ] No system audio capture.
- [ ] Local output only; no upload or network dependency.
- [ ] Pause, resume, stop, and cancel work.
- [ ] Screen + webcam composition works.
- [ ] Audio/video sync and long-duration behavior validated.
- [ ] Existing files are never overwritten.
- [ ] UI stays responsive.
- [ ] Bounded memory and queues.
- [ ] OS permissions are respected.
- [ ] No first-party unsafe Rust code.
- [ ] Dependency and native plugin review completed.
- [ ] CI and security checks pass.
