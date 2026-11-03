# Framelet — Product & Technical Specification

**Status:** MVP specification  
**Product:** Framelet  
**Repository:** `Danny4927/framelet-recorder`  
**Language:** Rust  
**Platforms:** Windows 10/11 and Linux (Wayland; X11 subject to backend validation)  
**macOS:** Explicitly unsupported

---

## 1. Product definition

Framelet is a small, performant, local-first desktop recorder. It records screen, microphone, webcam, or screen plus webcam to a local file. The application must be simple enough that a user can start a recording with only a few deliberate interactions.

### 1.1 MVP scope

The MVP includes exactly these four modes:

- **Screen:** capture a user-selected display, window, or source supported by the OS/backend. Microphone audio is optional.
- **Microphone:** record the selected microphone to a local audio file.
- **Webcam:** record the selected camera. Microphone audio is optional.
- **Screen + webcam:** composite the camera into the screen video as a configurable picture-in-picture bubble. Microphone audio is optional.

System audio is not part of the MVP.

### 1.2 Explicitly out of scope

No cloud, network, upload, account, authentication, workspace, library, collaboration, comments, reactions, analytics, streaming, browser extension, mobile app, AI, transcription, subtitles, video editing, stitching, or in-app video player.

The application must not require an internet connection to record.

### 1.3 Product principles

1. Local-first and offline-capable.
2. Privacy by default; capture starts only after explicit user action.
3. Minimal UI and low resource consumption.
4. Rust for first-party application logic.
5. No first-party `unsafe` Rust code.
6. No bypassing ownership, borrowing, type, or OS permission mechanisms.
7. Bounded memory and explicit backpressure.
8. Errors are visible; recordings must never be silently reported as successful if data was lost or finalization failed.

---

## 2. Platform support

### 2.1 Windows

Target Windows 10/11. Prefer Windows Graphics Capture for user-mediated display/window capture. Evaluate the supported Windows API and crate maturity during the capture PoC. Respect Windows camera and microphone privacy controls.

### 2.2 Linux

Wayland is a first-class target and must use XDG Desktop Portal for user-mediated source selection/permission and PipeWire for capture streams. Do not bypass compositor or portal restrictions.

X11 may use a separate backend. X11 support must be documented separately because its security and capture model differs from Wayland.

### 2.3 macOS

Not supported. Do not add macOS-specific code, dependencies, CI targets, packaging, or documentation promising support.

### 2.4 Platform abstraction

Keep OS-specific capture behind internal Rust traits. The application core must not depend directly on a specific capture crate or native API.

---

## 3. Recommended technology direction

These are candidates, not unconditional dependency approvals. Pin exact versions only after a current review of crates.io, upstream release notes, security advisories, license, maintenance, native dependencies, and unsafe implementation.

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

Before adopting a dependency, document:
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

A dependency is not approved merely because it is popular or convenient.

---

## 4. User experience

### 4.1 Main window

The main window should be compact and uncluttered. It contains:
- Four large mode choices.
- Microphone selector and enable/disable control.
- Camera selector when relevant.
- Camera preview when relevant.
- Recording destination.
- A prominent Start Recording button.
- Minimal settings for output quality and webcam bubble.

Do not expose advanced codec or pipeline options in the primary view.

### 4.2 Recording flow

1. User chooses a mode.
2. Framelet enumerates available devices and supported formats.
3. User selects the capture source through the OS-provided picker where applicable.
4. Required permissions are requested by the OS.
5. Preview is shown for camera modes.
6. User explicitly starts recording.
7. Optional three-second countdown.
8. Recording status, elapsed time, Pause, Resume, and Stop are shown.
9. On stop, sources are closed, buffers drained, and the output container finalized.
10. The UI shows the resulting file path only after successful finalization.

The UI must remain responsive while recording. The application must not record before the user presses Start.

### 4.3 Recording controls

- Start
- Pause
- Resume
- Stop
- Cancel/discard
- Camera preview

Stop must be idempotent. Cancel must clearly warn that the current recording will be discarded.

### 4.4 Webcam bubble

For Screen + webcam:
- Default position: bottom-right.
- Default size: approximately 20% of output width.
- Default source aspect ratio preserved; crop/fit behavior is consistent.
- Configurable size: small, medium, large.
- Configurable corner: top-left, top-right, bottom-left, bottom-right.
- Optional preview mirroring must not mirror the screen.
- Bubble is composited into the output video.
- Dynamic repositioning during recording is not required for MVP.

### 4.5 Suggested defaults

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

Device-supported formats take precedence. The app must never claim a setting is active if the selected device/backend cannot provide it.

---

## 5. Architecture

Use a modular Rust workspace and a Rust-first application core. No backend or services.

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

Create workspace members only when they contain valid manifests and buildable code; do not commit a knowingly broken placeholder workspace.

### 5.1 Module responsibilities

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

UI code must not perform capture, encoding, disk I/O, or blocking device enumeration.

### 5.2 Capture interfaces

Define narrow, safe traits for capture lifecycle and events. Prefer event streams/channels over blocking UI calls. All frames must have clear ownership; no borrowed pointer may outlive the source buffer.

Frame metadata must include:
- Width and height.
- Stride/row pitch.
- Pixel format.
- Monotonic timestamp.
- Sequence number.
- Owned pixel data or a safe, lifetime-bound buffer abstraction.

Audio blocks must include:
- Sample rate.
- Channel count.
- Sample format.
- Monotonic timestamp/sample position.
- Owned or safely borrowed sample storage with explicit lifetime.

Validate all dimensions, strides, lengths, and arithmetic before allocating or processing buffers. Use checked arithmetic.

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

Additional typed errors include:
- PermissionDenied
- SourceSelectionCancelled
- DeviceUnavailable
- CaptureFailed
- EncoderFailed
- StorageFailed
- InsufficientDiskSpace
- UnsupportedFormat
- PipelineOverrun

Only the core owns the state machine. UI reflects state events and cannot mutate state independently.

### 5.4 Threading and backpressure

- UI event loop remains non-blocking.
- Screen capture, camera capture, media processing, and storage run outside the UI thread.
- Audio callbacks must be real-time safe: no blocking, filesystem/network I/O, mutex waits, or expensive allocations.
- Transfer audio using a bounded single-producer/single-consumer ring buffer where suitable.
- Use bounded channels for video and control events.
- Never allow queues to grow without limit.
- Video may drop stale frames under overload, but drops must be counted and surfaced.
- Audio loss must be detected and reported; never silently conceal it.
- Shutdown and cancellation must propagate to every worker and release all capture resources.

---

## 6. Media pipeline

### 6.1 Modes

- Screen: screen video plus optional microphone.
- Microphone: audio-only output.
- Webcam: camera video plus optional microphone.
- Screen + webcam: screen and camera composed into one video plus optional microphone.

### 6.2 Synchronization

Use a monotonic clock. At recording start define a common origin T0. Map source timestamps onto a shared presentation timeline. Do not use wall-clock time as the synchronization source.

Requirements:
- Preserve source timestamps where possible.
- Handle variable frame rate and dropped frames.
- Keep audio sample position and timestamps consistent.
- Remove paused duration from the final timeline.
- Detect timestamp discontinuities and clock drift.
- Test A/V sync over long recordings.

### 6.3 Encoding and containers

Preferred output:
- Video: MP4/H.264.
- Audio: AAC in MP4/M4A.
- Microphone-only: M4A/AAC.

Hardware encoding may be preferred when it is available, supported, and stable. A software encoder fallback is required. Never assume a hardware encoder exists.

The GStreamer pipeline and required plugins must be tested and packaged explicitly for each supported OS. Do not dynamically load arbitrary plugins from user-writable directories. A completed file must not be reported until the container is finalized successfully.

### 6.4 Compositing

For Screen + webcam, composite the camera into the screen frame. Preserve aspect ratio, use a deterministic fit/crop policy, and avoid unnecessary frame copies. GPU compositing may be used only through safe, reviewed APIs and only if it does not compromise portability or reliability.

---

## 7. Local storage

Default output directory:
- Windows: user's Videos/Framelet directory.
- Linux: user's Videos/Framelet directory, resolved using standard user-directory conventions.

Requirements:
- User can select another directory.
- Never overwrite an existing recording.
- Use collision-resistant generated filenames.
- Create temporary output files exclusively.
- Finalize and rename only after successful muxer/container close.
- Validate paths and handle symlinks carefully.
- Do not invoke a shell to create files or directories.
- Do not accept arbitrary path fragments from untrusted metadata.
- Restrict file permissions to the current user where supported.
- Detect low disk space and write failures.
- On startup, identify incomplete temporary recordings and offer safe recovery/discard where possible.

A crash or power failure cannot be guaranteed to preserve a valid MP4. Recovery behavior must be documented and tested.

---

## 8. Performance goals

These are targets to measure, not guarantees:

| Metric | Target |
|---|---|
| Idle RAM | <100 MB |
| 1080p recording RAM | Aim <300 MB |
| Output | Up to 1080p30 |
| UI responsiveness | <100 ms for ordinary interactions |
| Start latency | <2 seconds after source/permission selection, excluding OS picker delay |
| A/V sync | <100 ms under supported configurations |
| Memory growth | Bounded during long recordings |
| CPU | Hardware encoding preferred when available |

Measure full process RSS, including native libraries and plugins. Publish benchmark hardware, OS, codec, and settings. If a target cannot be met, document the result rather than hiding it.

---

## 9. Error handling

Errors must be typed, actionable, and visible. No silent failure.

Examples:
- Permission denied: explain which OS permission is required and where to enable it.
- Source selection cancelled: return to Ready without starting.
- Camera disconnected: stop safely or continue only if the active mode remains valid and the user is informed.
- Microphone disconnected: report the loss; do not silently produce a falsely complete recording.
- Disk full: stop safely, attempt finalization, preserve recoverable data.
- Encoder failure: stop pipeline and report whether output is recoverable.
- Queue overrun: count drops and surface degraded recording quality.

Avoid panics in production paths. Do not use `unwrap()` or `expect()` on fallible runtime operations.

---

## 10. Testing and acceptance

### 10.1 Unit tests

Test state transitions, path validation, filename collisions, settings validation, timestamp mapping, checked buffer calculations, queue limits, and error mapping.

### 10.2 Integration tests

On every supported OS:
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

MVP is complete only when all four modes produce valid local files, the app works offline, permissions are respected, no existing file is overwritten, UI remains responsive, memory remains bounded, errors are visible, and security/CI checks pass.

---

## 11. Implementation sequence

1. **Capture PoC:** independently verify screen, microphone, and webcam capture on Windows and Linux.
2. **Pipeline PoC:** combine sources, synchronize, encode, and finalize files.
3. **UI shell:** implement compact Slint UI and state display.
4. **Core integration:** connect modes and recording lifecycle.
5. **Storage hardening:** safe temp files, collision handling, recovery.
6. **Platform validation:** Wayland and X11 are tested separately.
7. **Security review:** dependencies, unsafe audit, native plugins, permissions.
8. **Performance review:** idle, 1080p recording, long-duration memory and sync.
9. **Packaging:** signed/verified Windows artifacts and Linux packages.

Do not build a polished UI before the capture and media pipeline PoCs have passed.

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
