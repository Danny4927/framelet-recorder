# Capture PoC — backend evaluation

**Status:** Milestone 1 portal lifecycle smoke test is implemented. Milestone 2 frame delivery is blocked pending a verified safe PipeWire receive path. No capture backend is approved for production.

The MVP remains limited to screen, microphone, webcam, and screen + webcam. No system audio, network upload, or macOS.

## Candidate review — 3 November 2026

| Capability | Candidate | Evidence / current assessment | Decision |
|---|---|---|---|
| Linux Wayland portal | lamco-wayland 0.8.4, portal-only feature | Portal manager/session API; MIT OR Apache-2.0; upstream lists GNOME, KDE, wlroots as tested | Isolated milestone-1 PoC; not production-approved |
| Linux Wayland frames | lamco-pipewire 0.8.4 | Exact tagged source inspection found `PipeWireManager::create_stream` drops the frame-channel sender (`let (_tx, rx) = ...`) and leaves a TODO to connect it to the PipeWire thread. The public `frame_receiver` therefore cannot receive frames through this path. Lower-level thread API contains explicit unsafe internals and raw-FD ownership handling. | Manager receiver rejected; lower-level safe API review required |
| Linux X11 screen | Separate X11 adapter | Different security/capture model; lamco-wayland is Wayland-only | Deferred |
| Windows screen | Windows Graphics Capture via safe Rust wrapper | Public safe API and FFI/unsafe boundary must be verified | Research/PoC pending |
| Microphone | CPAL 0.18.2 | Callback is high-priority; bounded handling required | Candidate; callback/ring-buffer PoC pending |
| Webcam | nokhwa 0.10.11 | Device, backend, permissions, and formats need validation | Candidate; device PoC pending |

### lamco-pipewire 0.8.4 source review

The review is pinned to upstream tag `v0.8.4`, not an unpinned branch:

- In `crates/lamco-pipewire/src/manager.rs`, `PipeWireManager::create_stream` creates a Tokio channel as `let (_tx, rx) = mpsc::channel(...)`, stores `rx`, and returns a stream handle. The sender is dropped immediately.
- The same method contains a TODO to wire the sender to the PipeWire-thread bridge.
- `frame_receiver` removes and returns that receiver. It cannot receive frames through this manager path because its sender has already been dropped.
- `crates/lamco-pipewire/src/pw_thread.rs` has a separate bounded synchronous frame channel and a lower-level receive path. It also contains explicit unsafe code, including an `unsafe impl Send` for a DMA-BUF mmap pointer wrapper. This is third-party code, not Framelet code, but it is a required safety-review boundary.
- The lower-level API has raw file-descriptor ownership semantics. Do not pass a portal FD into it until ownership transfer and drop/teardown behavior are verified against the exact pinned source.

**Decision:** Do not use `PipeWireManager::frame_receiver` as a working capture source. Do not report success from connection/stream creation alone. Do not write custom unsafe FFI. See [ADR-0007](architecture/adr/ADR-0007-lamco-pipewire-frame-delivery.md).

### Required next technical step

Review the exact public lower-level API in `PipeWireThreadManager`: FD ownership, stream creation/destruction, bounded receive channel, async wake-up/timeout behavior, and frame buffer variants. If it passes review, implement the adapter in the isolated PoC and convert only validated CPU-resident supported pixel formats to Framelet-owned `VideoFrame`. Reject unsupported DMA-BUF or pixel formats explicitly; do not reinterpret them as packed BGRA. If the safe API is insufficient, request an upstream fix or select another backend.

### Other dependency review status

The previous review of CPAL and nokhwa is preliminary documentation research, not a source-level unsafe audit or RustSec/lockfile review. Exact resolved transitive versions and native dependencies must be recorded after the PoC lockfile is generated.

## Wayland portal PoC — milestone 1

See [poc/wayland-portal](../poc/wayland-portal/README.md). The isolated package is outside the production workspace. It opens no dialog unless explicitly invoked with `--start`. It creates an XDG portal session, reports selected stream count, and drops the session. It does not consume PipeWire frames or save media.

Commands:

~~~bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start
~~~

## Framelet capture-api contract

The production workspace has safe owned `VideoFrame` and `AudioBlock` values, explicit source kinds, and checked validation for dimensions, stride, buffer length, sample rate, channel count, and interleaved audio sample count. The first video formats are packed BGRA8/RGBA8. Streaming transport, bounded ring-buffer implementation, PipeWire frame conversion, and source-clock mapping remain PoC work.

## Real-device matrix

| Test | Windows 10/11 | Linux Wayland GNOME | Linux Wayland KDE | Linux X11 |
|---|---:|---:|---:|---:|
| Portal/source selection | N/A | Pending | Pending | Deferred |
| Screen capture 1080p30 for 10 min | Pending | Pending | Pending | Deferred |
| Microphone enumeration and 48 kHz capture | Pending | Pending | Pending | Pending |
| Webcam enumeration and 720p30 capture | Pending | Pending | Pending | Pending |
| Device unplug, denied permission, cancellation | Pending | Pending | Pending | Deferred |
| Bounded memory and clean shutdown | Pending | Pending | Pending | Deferred |
| No first-party unsafe Rust | Required | Required | Required | Required |

A backend is adopted only after build, maintenance, license, RustSec, dependency, native-library, unsafe/FFI, real-device, and packaging review. Compilation alone is not a passing device test.
