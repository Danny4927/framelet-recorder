# ADR-0007 — Do not use the lamco-pipewire 0.8.4 manager frame receiver

**Status:** Accepted for PoC; production backend remains unapproved  
**Date:** 3 November 2026

## Context

ADR-0006 selected `lamco-wayland` 0.8.4 for an isolated portal-only smoke test. The next milestone is actual screen-frame delivery from the portal-provided PipeWire connection.

The crate's public `PipeWireManager` API advertises `frame_receiver(stream_id)`. A source inspection of the exact `v0.8.4` tag shows that `create_stream` creates a bounded Tokio channel but binds its sender as `_tx`, immediately dropping it. The receiver is stored and returned to callers, but no producer can send frames into that channel. The same method contains a TODO stating that the sender is not wired to the PipeWire-thread bridge.

The PipeWire thread implementation has a separate frame channel and a public thread-manager surface, but uses raw file-descriptor ownership internally and contains explicit unsafe implementations (including `unsafe impl Send` for a cached mmap pointer wrapper). Third-party unsafe code is not automatically disallowed, but it requires a source-level safety review; Framelet itself must remain `#![forbid(unsafe_code)]`.

## Decision

1. Do not integrate `PipeWireManager::frame_receiver` from lamco-pipewire 0.8.4 as a working frame source.
2. Do not treat successful `connect`, `create_stream`, or `frame_receiver` calls as evidence of frame delivery.
3. Do not patch around the missing sender with first-party unsafe code or an undocumented fork.
4. Continue evaluating the lower-level public thread-manager API only after verifying its exact FD ownership contract, frame wake-up/receive API, bounded-queue behavior, stream teardown, and supported pixel-buffer representations.
5. If that lower-level API cannot satisfy the safe ownership and lifecycle contract, request an upstream fix or select another maintained safe abstraction. Keep the Wayland frame backend unapproved until a real KDE/GNOME device test observes actual frames.

## Alternatives considered

- **Use `PipeWireManager::frame_receiver`:** rejected; the receiver's sender is dropped in the pinned source.
- **Write a direct PipeWire adapter with custom FFI:** rejected by Framelet's non-negotiable safety policy.
- **Use `PipeWireThreadManager` directly:** remains a PoC candidate, not yet accepted; requires review of FD transfer, public receive APIs, queue bounds, and dependency unsafe code.
- **Wait for upstream repair:** acceptable fallback if the lower-level API does not pass review.

## Consequences

- Portal selection remains a valid milestone-1 test, but milestone 2 is blocked on a verified frame-delivery path.
- The current PoC must not print or report a successful capture result merely because a PipeWire stream was created.
- The exact dependency source and reviewed version must be pinned; re-audit on every update.
- A successful CI compile remains distinct from actual frame delivery on KDE Plasma Wayland.

## Validation required before reconsidering

- Locked build against the exact reviewed crate version.
- Automated test proving the receive path obtains a frame or times out/fails explicitly; no silent empty receiver.
- Validate dimensions, stride, pixel format, buffer length, timestamp and sequence before creating Framelet-owned `VideoFrame`.
- Demonstrate bounded frame transport and a documented overflow/drop counter.
- Exercise cancellation, portal closure, stream removal, and manager shutdown.
- Inspect unsafe/FFI and FD ownership in the exact dependency source and run RustSec/license checks.
- Run the real-device matrix on KDE Plasma Wayland and record actual frame metadata.

## References

- [lamco-pipewire v0.8.4 manager.rs](https://github.com/lamco-admin/lamco-wayland/blob/v0.8.4/crates/lamco-pipewire/src/manager.rs)
- [lamco-pipewire v0.8.4 pw_thread.rs](https://github.com/lamco-admin/lamco-wayland/blob/v0.8.4/crates/lamco-pipewire/src/pw_thread.rs)
- [lamco-pipewire v0.8.4 frame.rs](https://github.com/lamco-admin/lamco-wayland/blob/v0.8.4/crates/lamco-pipewire/src/frame.rs)
