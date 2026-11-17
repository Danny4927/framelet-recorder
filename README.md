# Framelet

**A lightweight, privacy-first screen recorder built with Rust.**

Framelet is a local-first desktop application for Windows and Linux. It focuses on four recording modes: screen, microphone-only, webcam, and screen with webcam overlay.

Framelet excludes cloud services, accounts, uploads, collaboration, AI processing, and in-app video editing from its MVP. System audio and macOS are out of scope.

## Project goals

- Minimal desktop UI and low resource use.
- Local recordings; no network dependency.
- Explicit OS-mediated capture permissions.
- Rust-first implementation; no first-party `unsafe` code.
- Audited and deliberately limited dependencies.

## Status

The repository contains safe capture data contracts, a bounded worker-level audio block queue, and isolated Wayland/CPAL capture proof-of-concepts. The queue is not callback-safe; CPAL remains outside the production workspace. Capture backends still require real-device, security and packaging validation.

## Documentation

- [Architecture](docs/architecture/README.md)
- [Specification](SPECIFICATION.md)
- [Security](SECURITY.md)
- [Capture backend PoCs](docs/CAPTURE_POC.md)
- [Microphone PoC guide](poc/microphone-cpal/README.md)
- [Linux installation](docs/LINUX_INSTALLATION.md)
- [Real-device testing](docs/REAL_DEVICE_TESTING.md)

## License

TBD. No license is granted until a license file is added.

## Development

Requires Rust 1.92 or newer and the native dependencies documented for each PoC.

```bash
cargo build --workspace
cargo test --workspace
```
