# Wayland portal + PipeWire frame PoC — milestone 2

This isolated PoC exercises the XDG Desktop Portal source picker and receives actual CPU-backed frames through PipeWire. It is deliberately outside the production Cargo workspace and does not add capture dependencies to Framelet itself.

## Requirements

- Linux running a Wayland compositor (not X11).
- A running XDG Desktop Portal backend (KDE: `xdg-desktop-portal` and `xdg-desktop-portal-kde`).
- PipeWire runtime and development libraries installed.
- Rust 1.92+.

The PoC uses the portal-only feature of `lamco-wayland 0.8.4` and the lower-level `PipeWireThreadManager` from `lamco-pipewire 0.8.4`. It deliberately does not use `PipeWireManager::frame_receiver`: in the pinned source that API drops its frame sender in `create_stream`.

## Run

From the repository root:

~~~bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo test --manifest-path poc/wayland-portal/Cargo.toml
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start --frames 120
~~~

The `--start` flag opens the OS source/permission dialog. Select a monitor or window. `--frames` controls how many valid frames must arrive before the PoC exits (default 120). Each wait has a five-second timeout. A timeout or five rejected frames is a failure, not a successful empty capture.

## Scope and safety

- Captures only the first stream returned by the portal.
- Requests CPU-backed buffers (`with_dmabuf(false)`); DMA-BUF frames are rejected explicitly.
- Accepts BGRA, BGRx, RGBA, and RGBx packed 32-bit buffers; converts them to owned Framelet BGRA frames.
- Checks dimensions, stride, checked buffer-size arithmetic, and actual byte length before conversion and again through `framelet-capture-api`.
- Uses the PipeWire library's bounded frame queue; no unbounded frame queue is introduced here.
- Does not write captured pixels to disk or create a recording.
- First-party Rust remains `#![forbid(unsafe_code)]`. `lamco-pipewire` contains third-party unsafe/FFI internals; this is an explicitly reviewed dependency boundary, not a claim that the dependency graph is unsafe-free.

The portal session must remain alive for the entire stream. The portal crate documents that the PipeWire FD ownership is transferred to its session handle and then to the PipeWire thread manager. The exact upstream source is pinned at v0.8.4 in [ADR-0007](../docs/architecture/adr/ADR-0007-lamco-pipewire-frame-delivery.md).

## What this PoC does not prove

A CI build or unit test does not prove real screen capture. Actual KDE Plasma Wayland testing is still required for source selection, frame delivery, cancellation, session closure, display removal, sustained capture, CPU/memory use, and shutdown. Do not use this as a production backend yet.
