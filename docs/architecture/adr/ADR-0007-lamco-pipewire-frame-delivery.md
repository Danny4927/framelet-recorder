# ADR-0007 — PipeWire frame delivery through the lower-level thread manager

**Status:** Accepted for isolated PoC; production backend remains unapproved  
**Date:** 3 November 2026

## Context

ADR-0006 selected `lamco-wayland` 0.8.4 for an isolated portal smoke test. Source inspection of the exact upstream `v0.8.4` tag showed that `PipeWireManager::create_stream` creates a Tokio channel but binds its sender as `_tx`, immediately dropping it. Its `frame_receiver` cannot deliver frames through this path.

The same release exposes `PipeWireThreadManager`, a lower-level API with a bounded synchronous frame channel, `try_recv_frame`, `recv_frame_timeout`, and `frame_notify`. It confines PipeWire's non-Send objects to a dedicated thread. The implementation contains third-party unsafe/FFI internals, including raw-FD reconstruction and unsafe Send implementations for an mmap pointer wrapper.

The portal session handle documents that its PipeWire FD ownership is transferred to the session handle (not closed when that handle drops) and then to the PipeWire thread manager. The thread manager accepts a `RawFd` and reconstructs an `OwnedFd` internally. This ownership transfer must remain explicit and the portal session must remain alive while capture is active.

## Decision

1. Do not use `PipeWireManager::frame_receiver` from lamco-pipewire 0.8.4.
2. Use `PipeWireThreadManager` only inside the isolated Linux PoC, after explicit source review of its FD ownership, bounded receive API, stream lifecycle, and third-party unsafe boundary.
3. Request CPU-backed buffers and reject DMA-BUF frames in this PoC. Do not attempt to map GPU memory in Framelet.
4. Accept only packed BGRA, BGRx, RGBA, and RGBx 32-bit frames. Convert to owned Framelet BGRA frames with checked dimensions, stride, buffer length, and arithmetic. Reject all other formats explicitly.
5. Keep all first-party Rust under `#![forbid(unsafe_code)]`. Do not add handwritten FFI, raw pointers, or unsafe wrappers.
6. Keep the backend unapproved for production until CI, RustSec/license review, actual KDE/GNOME device tests, teardown, and sustained capture tests pass.

## Alternatives considered

- **Use `PipeWireManager::frame_receiver`:** rejected; sender is dropped in the pinned source.
- **Use `PipeWireThreadManager`:** accepted for isolated PoC only; has the necessary bounded frame receive API but carries a third-party unsafe/FD ownership boundary.
- **Write direct PipeWire FFI in Framelet:** rejected by the project's safety policy.
- **Wait for upstream manager repair:** remains an option if lower-level lifecycle or dependency review fails.

## Consequences

- The PoC can now attempt real frame delivery without adding first-party unsafe Rust.
- The dependency graph is not claimed to be unsafe-free. The lower-level crate's unsafe internals require version-pinned review.
- The PoC intentionally handles CPU buffers only; DMA-BUF and non-packed formats are unsupported.
- No recording file is produced and no production capture adapter is added.
- CI success is not equivalent to a successful KDE desktop capture test.

## Validation required before production adoption

- Locked build and committed PoC Cargo.lock.
- Automated conversion/layout tests and CI.
- RustSec, license, native library, transitive dependency, and source-level unsafe/FFI review.
- Actual KDE Plasma Wayland test proving frames arrive and are validated.
- Cancellation, portal closure, stream removal, display unplug, and manager shutdown tests.
- Ten-minute sustained capture with bounded memory, frame-rate/drop metrics, and clean shutdown.
- A/V clock mapping and encoder integration are separate future milestones.

## References

- [lamco-pipewire v0.8.4 manager.rs](https://github.com/lamco-admin/lamco-wayland/blob/v0.8.4/crates/lamco-pipewire/src/manager.rs)
- [lamco-pipewire v0.8.4 pw_thread.rs](https://github.com/lamco-admin/lamco-wayland/blob/v0.8.4/crates/lamco-pipewire/src/pw_thread.rs)
- [lamco-portal v0.8.4 session.rs](https://github.com/lamco-admin/lamco-wayland/blob/v0.8.4/crates/lamco-portal/src/session.rs)
