# Capture PoC — backend evaluation

**Status:** Wayland milestone 3 adds a bounded-duration PipeWire frame run, process-side diagnostics, and explicit worker/session shutdown in the isolated PoC. Real KDE/GNOME device validation and dependency/security gates remain open. No capture backend is approved for production.

The MVP remains limited to screen, microphone, webcam, and screen + webcam. No system audio, network upload, or macOS.

## Candidate review — 3 November 2026

| Capability | Candidate | Evidence / current assessment | Decision |
|---|---|---|---|
| Linux Wayland portal | lamco-wayland 0.8.4, portal-only feature | Portal manager/session API; MIT OR Apache-2.0; upstream lists GNOME, KDE, wlroots as tested | Isolated PoC |
| Linux Wayland frames | lamco-pipewire 0.8.4 | Lower-level `PipeWireThreadManager` receives through a bounded queue; uses raw-FD ownership and third-party unsafe internals documented in ADR-0007 | Isolated PoC; production approval pending |
| Linux X11 screen | Separate X11 adapter | Different security/capture model; lamco-wayland is Wayland-only | Deferred |
| Windows screen | Windows Graphics Capture via safe Rust wrapper | Public safe API and FFI/unsafe boundary must be verified | Research/PoC pending |
| Microphone | CPAL 0.18.2 | Callback is high-priority; bounded handling required | Candidate; callback/ring-buffer PoC pending |
| Webcam | nokhwa 0.10.11 | Device, backend, permissions, and formats need validation | Candidate; device PoC pending |

## Wayland milestone 3 — bounded capture run and lifecycle diagnostics

The isolated `poc/wayland-portal` package now:

1. Opens the XDG portal source picker only after explicit `--start`.
2. Keeps the portal session alive while the PipeWire worker owns the portal-provided FD.
3. Captures CPU-backed frames through the lower-level bounded PipeWire queue.
4. Rejects DMA-BUF, corrupted/incomplete, unsupported formats, invalid dimensions/stride, and short buffers.
5. Converts supported packed 32-bit pixels to owned Framelet BGRA frames.
6. Runs until the first of a frame target or configured duration is reached (default: ten minutes).
7. Emits periodic and final process-side FPS, sequence-gap, timestamp-regression, rejected/corrupted-buffer, and frame-byte diagnostics.
8. Shuts down PipeWire before dropping the portal session on normal completion and handled frame/timeout errors.

Commands:

```bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo test --manifest-path poc/wayland-portal/Cargo.toml
cargo clippy --manifest-path poc/wayland-portal/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start --frames 120
```

FPS is the process-side receive rate, sequence gaps are best-effort diagnostics, and frame-byte totals are throughput rather than process RSS. This PoC still does not write media, so it does not prove encoding or recording.

## Dependency and safety boundary

- First-party Rust remains `#![forbid(unsafe_code)]`.
- `lamco-pipewire` contains third-party unsafe/FFI code and takes ownership of a raw FD; see [ADR-0007](architecture/adr/ADR-0007-lamco-pipewire-frame-delivery.md).
- The PipeWire frame queue is bounded by the dependency; no second unbounded queue is introduced.
- Only CPU-resident packed 32-bit formats are accepted.
- Exact crate versions remain pinned. The standalone PoC lockfile and RustSec/license/source audit are required before production consideration.
- CI compile/tests cannot validate actual compositor behavior or process RSS.

## Remaining real-device evidence

| Scenario | Required observation | Current status |
|---|---|---|
| KDE monitor selection | Native chooser and validated frames | Not run |
| KDE window selection | Window frames where offered | Not run |
| GNOME selection | Portal selection and validated frames | Not run |
| Cancel/deny chooser | Clean cancellation and no leaked session/worker | Not run |
| Ten-minute capture | Sustained reception, stable timestamps, bounded RSS | Not run |
| PipeWire/portal closure | Clean stop and resource release | Not run |
| Display unplug/reconfigure | Typed failure/recovery without crash | Not run |
| CPU/frame cadence | FPS and frame-byte diagnostics recorded | Not run |

A backend is adopted only after build, maintenance, license, RustSec, dependency, native-library, unsafe/FFI, real-device, and packaging review. Compilation alone is not a passing device test.
