# Wayland portal + PipeWire capture-lifecycle PoC — milestone 3

This isolated PoC exercises XDG Desktop Portal source selection, receives CPU-backed PipeWire frames, validates/converts them into owned Framelet frames, reports capture statistics, and shuts down the PipeWire worker before releasing the portal session.

It remains outside the production Cargo workspace. It does not encode, save, or otherwise persist captured pixels.

## Requirements

- Linux running a Wayland compositor (not X11).
- A running XDG Desktop Portal backend (KDE: `xdg-desktop-portal` and `xdg-desktop-portal-kde`).
- PipeWire runtime and development libraries installed.
- Rust 1.92+.

The PoC pins `lamco-wayland 0.8.4` and `lamco-pipewire 0.8.4`. It uses the lower-level `PipeWireThreadManager`; it does not use the known broken `PipeWireManager::frame_receiver` channel.

## Run

From the repository root:

```bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo test --manifest-path poc/wayland-portal/Cargo.toml
cargo clippy --manifest-path poc/wayland-portal/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start
```

The `--start` flag is required to open the OS source/permission dialog. It runs for up to ten minutes by default, then shuts down cleanly. Use:

- `--frames N` to stop after N valid frames.
- `--duration-seconds N` to set a shorter/longer time limit.
- When both are supplied, the first reached limit stops capture.

For example, a quick smoke test is:

```bash
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start --frames 120
```

## Statistics

Progress is printed every five seconds and at shutdown:

- elapsed duration and validated/rejected frame counts;
- received frames per second;
- sequence gaps (best-effort indication of missing frames);
- timestamp regressions;
- PipeWire-reported corrupted buffers;
- cumulative owned frame bytes and maximum frame size.

The reported FPS is a process-side receive rate, not a guarantee of compositor output FPS. Sequence gaps depend on upstream frame identifiers and should be interpreted as diagnostic evidence, not a definitive PipeWire drop count. Memory figures are frame-byte throughput/high-water values, not process RSS.

## Lifecycle and limitations

- A five-second frame wait timeout is a hard failure unless the configured duration has elapsed.
- Five rejected frames stop the PoC.
- On normal completion and handled receive/rejection errors, the PipeWire thread is shut down before the portal session is dropped.
- The bounded frame queue belongs to the pinned PipeWire dependency; the PoC adds no unbounded queue.
- Only the first stream is captured.
- CPU-backed BGRA, BGRx, RGBA, and RGBx are supported; DMA-BUF and other formats are rejected.
- No microphone, webcam, muxing, file output, pause/resume, or A/V synchronization is implemented.
- Rust code in this PoC forbids first-party unsafe code. Third-party unsafe/FFI internals still require dependency review.

This is not production-backend approval. CI and unit tests do not replace a real KDE/GNOME Wayland test.
