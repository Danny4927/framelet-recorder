# Framelet

**A lightweight, privacy-first screen recorder built with Rust.**

Framelet is a local-first desktop application for Windows and Linux. It focuses on four recording modes:

1. Screen recording
2. Microphone-only recording
3. Webcam recording
4. Screen recording with a webcam overlay

Framelet intentionally excludes cloud services, accounts, uploads, collaboration features, AI processing, and in-app video editing from its MVP.

## Project goals

- Minimal, easy-to-use desktop UI.
- Low idle and recording resource usage.
- Local recording files; no network dependency.
- Explicit, operating-system-mediated capture permissions.
- Rust-first implementation.
- No `unsafe` in first-party Rust code.
- Audited and deliberately limited dependencies.

## Supported platforms

- **Windows 10/11** — primary target.
- **Linux** — Wayland is a first-class target; X11 support is separately evaluated.

**macOS is explicitly out of scope and will not be supported.**

## MVP recording modes

| Mode | Video | Audio |
|---|---|---|
| Screen | Selected display/window/source | Optional microphone |
| Microphone | — | Selected microphone |
| Webcam | Selected camera | Optional microphone |
| Screen + webcam | Screen with composited camera bubble | Optional microphone |

System audio capture is out of scope.

## Status

The repository contains the product and engineering specification, the Recording core/use cases and ports, safe media data contracts, a bounded worker-level audio block queue, and isolated Wayland, CPAL microphone, and GStreamer media proof-of-concepts. CPAL and GStreamer remain outside the production workspace. No production media backend is approved; capture and media PoCs still require real-device, independent playback, security and packaging validation.

## Documentation

- [Architecture (arc42 + ADRs)](docs/architecture/README.md)
- [Product and technical specification](SPECIFICATION.md)
- [Security policy and engineering rules](SECURITY.md)
- [Capture backend PoCs](docs/CAPTURE_POC.md)\n- [GStreamer media pipeline PoC](poc/media-gstreamer/README.md)
- [Microphone PoC guide](poc/microphone-cpal/README.md)
- [Linux installation plan (Arch Linux / CachyOS)](docs/LINUX_INSTALLATION.md)
- [Real-device testing](docs/REAL_DEVICE_TESTING.md)
- [Contributor and AI-agent instructions](AGENTS.md)

## License

TBD. No license is granted until a license file is added.

## Development workspace

The Rust workspace and Slint desktop shell provide the current development foundation. Real media capture, encoding and storage finalization have not been integrated into the production application.

Requires Rust 1.92 or newer and the native dependencies documented for the applicable PoC.

```bash
cargo build --workspace
cargo test --workspace
```
