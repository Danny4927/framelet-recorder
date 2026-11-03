# Framelet

A lightweight, privacy-first screen recorder built with Rust.

**Capture. Save. Done.**

Framelet is a local-first desktop recorder for Windows and Linux. The MVP is limited to screen,
microphone, webcam, and screen + webcam recording. macOS is not supported.

## Project status

The repository now has an initial Rust workspace, a minimal Slint desktop shell, safe capture
contracts, and a capture-backend evaluation. Real device capture is still a PoC task and is not
yet implemented.

## Build

Requires Rust 1.92 or newer (stable), plus native build dependencies required by Slint.

```bash
cargo build --workspace
cargo test --workspace

Visual snapshots (after creating the initial baseline):

FRAMELET_RUN_VISUAL_TESTS=1 cargo test -p framelet-desktop --test visual_regression
```

See SPECIFICATION.md, SECURITY.md, AGENTS.md, docs/CAPTURE_POC.md, and docs/VISUAL_TESTING.md.
No cloud, account, upload, or network service is part of the product.
