# Capture PoC — backend evaluation

**Status:** Milestone 2 implementation added in an isolated PoC: portal selection plus CPU-backed PipeWire frame reception and validation. Compilation, automated tests, and real KDE device behavior must be verified before considering the backend for production.

The MVP remains limited to screen, microphone, webcam, and screen + webcam. No system audio, network upload, or macOS.

## Candidate review — 3 November 2026

| Capability | Candidate | Evidence / current assessment | Decision |
|---|---|---|---|
| Linux Wayland portal | lamco-wayland 0.8.4, portal-only feature | Portal manager/session API; MIT OR Apache-2.0; upstream lists GNOME, KDE, wlroots as tested | Isolated PoC |
| Linux Wayland frames | lamco-pipewire 0.8.4 | Manager receiver rejected: `create_stream` drops its sender. Lower-level `PipeWireThreadManager` provides bounded frame channel, explicit receive methods, and a dedicated PipeWire thread. Its raw-FD ownership and third-party unsafe internals are documented in ADR-0007. | Isolated milestone-2 PoC; production approval pending |
| Linux X11 screen | Separate X11 adapter | Different security/capture model; lamco-wayland is Wayland-only | Deferred |
| Windows screen | Windows Graphics Capture via safe Rust wrapper | Public safe API and FFI/unsafe boundary must be verified | Research/PoC pending |
| Microphone | CPAL 0.18.2 | Callback is high-priority; bounded handling required | Candidate; callback/ring-buffer PoC pending |
| Webcam | nokhwa 0.10.11 | Device, backend, permissions, and formats need validation | Candidate; device PoC pending |

## Wayland milestone 2 — PipeWire frame reception

The isolated `poc/wayland-portal` package now:

1. Opens the XDG portal source picker on explicit `--start`.
2. Keeps the portal session alive and selects its first returned stream.
3. Transfers the portal-provided PipeWire FD to `PipeWireThreadManager` according to the upstream ownership contract.
4. Requests CPU-backed buffers and creates one PipeWire stream.
5. Receives frames through the lower-level manager's bounded frame queue with a five-second per-frame timeout.
6. Rejects DMA-BUF, corrupted/incomplete, unsupported formats, invalid dimensions/stride, and short buffers.
7. Converts BGRA/BGRx/RGBA/RGBx packed 32-bit pixels into owned Framelet BGRA `VideoFrame` values and validates them through `framelet-capture-api`.
8. Stops after the requested number of frames and shuts down the PipeWire thread.

Run:

~~~bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo test --manifest-path poc/wayland-portal/Cargo.toml
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start --frames 120
~~~

The default target is 120 validated frames. This is a frame-delivery smoke test only: it does not mux or save a recording, does not implement long-running cancellation, and does not establish A/V clock synchronization. See [the PoC README](../poc/wayland-portal/README.md).

## Dependency and safety boundary

- Framelet first-party code remains `#![forbid(unsafe_code)]`.
- `lamco-pipewire` contains third-party unsafe/FFI code. The direct manager API uses a raw FD and reconstructs ownership internally; this depends on the portal library's documented transfer semantics. This is a dependency safety boundary, not a claim that the dependency graph is unsafe-free.
- The PipeWire frame queue is bounded by the dependency. The PoC does not add another unbounded queue.
- Only CPU-resident packed 32-bit formats are accepted. DMA-BUF and non-packed formats fail explicitly.
- The exact upstream source version is pinned at `v0.8.4`; update/review it before changing the version.
- The PoC does not yet commit a generated Cargo.lock. Locked/reproducible build and RustSec/license checks remain release gates.

## Real-device matrix

| Test | Windows 10/11 | Linux Wayland GNOME | Linux Wayland KDE | Linux X11 |
|---|---:|---:|---:|---:|
| Portal/source selection | N/A | Pending | Pending | Deferred |
| Actual frame delivery, 120 frames | N/A | Pending | Pending | Deferred |
| Screen capture 1080p30 for 10 min | Pending | Pending | Pending | Deferred |
| Microphone enumeration and 48 kHz capture | Pending | Pending | Pending | Pending |
| Webcam enumeration and 720p30 capture | Pending | Pending | Pending | Pending |
| Device unplug, denied permission, cancellation | Pending | Pending | Pending | Deferred |
| Bounded memory and clean shutdown | Pending | Pending | Pending | Deferred |
| No first-party unsafe Rust | Required | Required | Required | Required |

A backend is adopted only after build, maintenance, license, RustSec, dependency, native-library, unsafe/FFI, real-device, and packaging review. Compilation alone is not a passing device test.
