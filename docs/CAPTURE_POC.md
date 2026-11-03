# Capture PoC — backend evaluation

**Status:** Evaluation started; no capture backend is approved for production yet.

The MVP remains limited to screen, microphone, webcam, and screen + webcam. No system audio,
network upload, or macOS.

## Candidate status — 3 November 2026

| Capability | Candidate | Initial assessment | Decision |
|---|---|---|---|
| Linux Wayland screen | XDG Desktop Portal + PipeWire; lamco-wayland 0.8.1 | Permission model fits; requires native PipeWire libraries | Prototype behind VideoSource; validate GNOME and KDE |
| Linux X11 screen | Separate X11 adapter | Different security/capture model | Defer until Wayland passes |
| Windows screen | Windows Graphics Capture via safe Rust wrapper | Native API fits; wrapper FFI/unsafe boundary needs review | Compare safe API, maintenance, packaging |
| Microphone | CPAL 0.18.2 | Cross-platform audio I/O; callback must be bounded and real-time safe | Prototype device enumeration and input |
| Webcam | nokhwa 0.10.11 | Lists Media Foundation on Windows and Video4Linux on Linux | Prototype formats, unplug, permissions |

This is not a full security approval. Review transitive dependencies, licenses, native libraries,
and unsafe internals before adding capture crates to the production dependency graph.

## Real-device matrix

| Test | Windows 10/11 | Linux Wayland | Linux X11 |
|---|---:|---:|---:|
| Enumerate displays/windows and user picker | Pending | Pending | Deferred |
| Screen capture at 1080p30 for 10 minutes | Pending | Pending | Deferred |
| Microphone enumeration and 48 kHz capture | Pending | Pending | Pending |
| Webcam enumeration and 720p30 capture | Pending | Pending | Pending |
| Device unplug, denied permission, cancellation | Pending | Pending | Pending |
| Bounded memory and clean shutdown | Pending | Pending | Pending |
| No first-party unsafe Rust | Required | Required | Required |

A backend is adopted only after build, maintenance, license, RustSec, dependency, native-library,
and unsafe/FFI review. Permission prompts must be OS-mediated; cancellation, unsupported formats,
and disconnects must return typed errors. Compilation alone is not a passing device test.
