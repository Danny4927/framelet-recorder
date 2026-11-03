# Wayland portal PoC — milestone 1

This is an isolated, opt-in smoke test. It is deliberately outside the production Cargo workspace and does not add a capture dependency to Framelet itself.

## Requirements

- Linux running a Wayland compositor (not X11).
- A running XDG Desktop Portal backend.
- PipeWire runtime available for the subsequent streaming milestone.
- Rust 1.92+.

The initial smoke test uses the portal-only feature of lamco-wayland 0.8.4. Upstream documents portal manager/session creation and identifies GNOME, KDE Plasma, and wlroots portal backends as tested. X11 is not supported by this library.

## Run

From the repository root:

~~~bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start
~~~

The --start flag is required because execution triggers an OS permission/source-selection dialog. Without it, the program only prints usage. Choose a source in the OS dialog. Closing the dialog without granting access should return an error/cancellation result; record the exact behavior and verify no session remains active.

The smoke test creates a portal session, prints the number of selected streams, then drops the session. It does not read frame buffers, write files, record audio, or contact any Framelet server (there is no server).

## Milestone 1 checklist

- [ ] Builds on a clean Ubuntu/Fedora Wayland development environment.
- [ ] GNOME portal session accepted.
- [ ] KDE portal session accepted.
- [ ] User cancels picker; cancellation is distinguishable from permission denial.
- [ ] Session is released on normal return and error paths.
- [ ] Dependency license, transitive dependencies, RustSec and unsafe/FFI audit recorded.

## Milestone 2 — next

Integrate PipeWire frame delivery behind a Framelet-owned adapter. Validate actual buffer format/stride, timestamps, cursor behavior, frame rate, bounded buffering, device/session teardown, and conversion to owned Framelet frames. Do not proceed to production integration until the no-unsafe boundary and native dependency review pass.
