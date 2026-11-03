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

The repository currently contains the product and engineering specification. Capture-library choices and codec pipelines must pass the proof-of-concept and security gates described in [SPECIFICATION.md](SPECIFICATION.md) before being treated as production decisions.

## Documentation

- [Product and technical specification](SPECIFICATION.md)
- [Security policy and engineering rules](SECURITY.md)
- [Contributor and AI-agent instructions](AGENTS.md)

## License

TBD. No license is granted until a license file is added.
