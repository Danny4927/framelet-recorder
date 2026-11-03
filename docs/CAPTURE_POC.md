# Capture PoC — backend evaluation

**Status:** Milestone 1 started: isolated Wayland portal session smoke test. No capture backend is approved for production.

The MVP remains limited to screen, microphone, webcam, and screen + webcam. No system audio, network upload, or macOS.

## Candidate review — 3 November 2026

| Capability | Candidate | Evidence / current assessment | Decision |
|---|---|---|---|
| Linux Wayland portal | lamco-wayland 0.8.4, portal-only feature | Current docs show portal manager/session API; MIT OR Apache-2.0; GNOME, KDE, wlroots listed as tested upstream | Isolated PoC started; not production-approved |
| Linux Wayland frames | lamco-wayland PipeWire integration | Must inspect exact stream/buffer APIs, timestamps, format/stride, lifecycle and unsafe boundary | Milestone 2; not started |
| Linux X11 screen | Separate X11 adapter | Different security/capture model; lamco-wayland is Wayland-only | Deferred |
| Windows screen | Windows Graphics Capture via safe Rust wrapper | Public safe API and FFI/unsafe boundary must be verified | Research/PoC pending |
| Microphone | CPAL 0.18.2 | Current docs list Windows WASAPI and Linux ALSA/JACK/PipeWire/PulseAudio; callback is high-priority and requires bounded handling; Apache-2.0 | Candidate; callback/ring-buffer PoC pending |
| Webcam | nokhwa 0.10.11 | Current docs expose Camera/query and native input feature; backend/permissions/formats need validation | Candidate; device PoC pending |

### Current upstream evidence

- lamco-wayland 0.8.x docs: portal session API, portal-only feature, MIT/Apache-2.0, Rust 1.87 MSRV, PipeWire system dependency only when PipeWire feature is enabled. Upstream lists GNOME, KDE Plasma, and wlroots as tested and explicitly says X11 is unsupported.
- CPAL 0.18.2 docs: cross-platform device/stream API, platform host coverage, callback runs on a dedicated high-priority thread; Linux ALSA development files are required for its default Linux build.
- nokhwa 0.10.11 docs: Camera abstraction and native input feature; this is not yet a platform compatibility or security approval.

This is an initial documentation review, not a full source-level unsafe audit or RustSec/lockfile review. Exact resolved transitive versions and native dependencies must be recorded after the PoC lockfile is generated.

## Wayland portal PoC — milestone 1

See [poc/wayland-portal](../poc/wayland-portal/README.md). The isolated package is outside the production workspace. It opens no dialog unless explicitly invoked with --start. It creates an XDG portal session, reports selected stream count, and drops the session. It does not yet consume PipeWire frames or save media.

Commands:

~~~bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start
~~~

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
