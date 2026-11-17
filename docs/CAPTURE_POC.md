# Capture PoC — backend evaluation

**Status:** Wayland milestone 3 adds a bounded-duration PipeWire frame run, process-side diagnostics, and explicit worker/session shutdown in the isolated PoC. The CPAL microphone milestone adds isolated device enumeration and bounded F32 sample reception. Real KDE/GNOME and microphone device validation, dependency/security gates, and production integration remain open. No capture backend is approved for production.

The MVP remains limited to screen, microphone, webcam, and screen + webcam. No system audio, network upload, or macOS.

## Candidate review — 17 November 2026

| Capability | Candidate | Evidence / current assessment | Decision |
|---|---|---|---|
| Linux Wayland portal | lamco-wayland 0.8.4, portal-only feature | Portal manager/session API; MIT OR Apache-2.0; upstream lists GNOME, KDE, wlroots as tested | Isolated PoC |
| Linux Wayland frames | lamco-pipewire 0.8.4 | Lower-level `PipeWireThreadManager` receives through a bounded queue; uses raw-FD ownership and third-party unsafe internals documented in ADR-0007 | Isolated PoC; production approval pending |
| Linux X11 screen | Separate X11 adapter | Different security/capture model; lamco-wayland is Wayland-only | Deferred |
| Windows screen | Windows Graphics Capture via safe Rust wrapper | Public safe API and FFI/unsafe boundary must be verified | Research/PoC pending |
| Microphone | CPAL 0.18.2 | Apache-2.0; safe input stream API; dedicated high-priority callback; isolated F32-only PoC uses a fixed-capacity SPSC ring. Third-party native/unsafe boundary and advisories remain to be reviewed. | Isolated evaluation PoC; production approval pending |
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

## Microphone milestone 1 — CPAL device and F32 input PoC

The standalone `poc/microphone-cpal` package is outside the production workspace and does not add CPAL to the production dependency graph.

- `--list` enumerates local input devices; no stream starts.
- `--record --device INDEX` explicitly starts the chosen device's default input stream; default duration is ten seconds and can be changed with `--duration-seconds N`.
- Only a default F32 input format is accepted for this first milestone. Other formats fail explicitly rather than being silently converted.
- The callback copies into a preallocated two-second SPSC sample ring. It does not allocate, lock, sleep, log, send a channel message, or perform filesystem/network I/O.
- Samples that do not fit are counted and dropped; the consumer never has unread samples overwritten.
- The stream is explicitly started with CPAL's `.play()` call. On stop, the PoC drains/counts buffered samples and discards them. **No audio file is created.**

Run from the repository root:

```bash
cargo check --manifest-path poc/microphone-cpal/Cargo.toml
cargo test --manifest-path poc/microphone-cpal/Cargo.toml
cargo clippy --manifest-path poc/microphone-cpal/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --list
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --record --device 0 --duration-seconds 10
```

The device index is local to the current enumeration and is not a persistent device identifier. Linux explicitly enables CPAL's PipeWire feature; ALSA is compiled as a target-specific backend dependency. Windows WASAPI and Linux PipeWire/ALSA paths still need real-device validation.

## Dependency and safety boundary

- First-party Rust remains `#![forbid(unsafe_code)]`.
- `lamco-pipewire` contains third-party unsafe/FFI code and takes ownership of a raw FD; see [ADR-0007](architecture/adr/ADR-0007-lamco-pipewire-frame-delivery.md).
- The CPAL input stream uses a safe public Rust API, but platform backends may include third-party unsafe/FFI internals; see [ADR-0008](architecture/adr/ADR-0008-cpal-microphone-capture.md).
- The PipeWire and microphone PoCs use bounded buffering. Exact native dependency/runtime requirements and measured device behavior remain platform gates.
- Exact crate versions are pinned in standalone manifests. Committed lockfiles, RustSec, license/source, transitive dependency, and native/unsafe review remain required before production consideration.
- CI compile/tests do not validate actual compositor or microphone device behavior.

## Remaining real-device evidence

| Scenario | Required observation | Current status |
|---|---|---|
| KDE monitor selection | Native chooser and validated frames | Not run |
| KDE window selection | Window frames where offered | Not run |
| GNOME selection | Portal selection and validated frames | Not run |
| Cancel/deny chooser | Clean cancellation and no leaked session/worker | Not run |
| Ten-minute screen capture | Sustained reception, stable timestamps, bounded RSS | Not run |
| PipeWire/portal closure | Clean stop and resource release | Not run |
| Linux PipeWire microphone | Input device listing, F32 sample delivery, stop, stable callback | Not run |
| Linux ALSA microphone | Input device listing, F32 sample delivery, stop, stable callback | Not run |
| Windows WASAPI microphone | Input device listing, F32 sample delivery, stop, stable callback | Not run |
| Microphone hot-unplug | Stream error observed; callback stops; no panic/leak | Not run |
| Ring-buffer stress | Bounded memory, overflow counted, no unread overwrite | Unit-test only; real-time stress not run |
| Display unplug/reconfigure | Failure is visible; no crash or FD leak | Not run |

A backend is adopted only after build, maintenance, license, RustSec, dependency, native-library, unsafe/FFI, real-device, and packaging review. Compilation alone is not a passing device test.
