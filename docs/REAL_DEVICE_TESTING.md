# Real-device testing — Linux Wayland / KDE Plasma

## Scope and honesty

A CI compile or unit test is not a real desktop capture test. These tests require a logged-in Wayland desktop, a working D-Bus user session, the KDE portal backend, PipeWire, and a user who can interact with the native source chooser. CI cannot claim these tests passed unless it runs that actual environment.

## Target environment

- Arch Linux or CachyOS, fully updated.
- KDE Plasma session running on Wayland (not X11/XWayland).
- `xdg-desktop-portal` and `xdg-desktop-portal-kde`.
- PipeWire and a working session manager (normally WirePlumber).
- Rust 1.92+ for running the isolated PoC from source.

Check the session and services:

```bash
printf 'session=%s desktop=%s wayland=%s\n' "$XDG_SESSION_TYPE" "$XDG_CURRENT_DESKTOP" "$WAYLAND_DISPLAY"
systemctl --user --no-pager status xdg-desktop-portal.service
systemctl --user --no-pager status pipewire.service
systemctl --user --no-pager status wireplumber.service
```

Expected: `XDG_SESSION_TYPE=wayland`, a KDE desktop identifier, and non-empty `WAYLAND_DISPLAY`. Services may be socket/D-Bus activated; a missing standalone service status is not by itself proof that the portal is broken.

## Milestone 1 — portal lifecycle smoke test

From the repository root:

```bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start
```

Test separately:

1. Select a monitor in the KDE chooser and accept.
2. Confirm the process reports a session and the selected stream count.
3. Confirm the process exits and the session is released.
4. Run again and close/cancel the chooser.
5. Run again and deny access if the backend exposes a distinct deny path.
6. Repeat with a window source if the KDE portal offers it.

Record Plasma, portal, PipeWire, and kernel versions, the exact terminal output, whether the chooser appeared, selected source type, and whether cancellation/denial was distinguishable. Do not attach screenshots containing private desktop content.

## Milestone 2 — real PipeWire frames

The current PoC does not yet consume PipeWire buffers. Do not mark this milestone passed based on the portal smoke test.

Required implementation and observations:

- connect using the PipeWire FD returned by the portal session;
- create one stream for each selected portal stream;
- receive actual frames and record width, height, pixel format, stride, timestamp, and sequence;
- validate every buffer before conversion to Framelet-owned `VideoFrame`;
- use a bounded channel/queue and document the overflow policy;
- stop on portal/session closure and release all streams/FDs;
- verify no first-party unsafe Rust and review all dependency unsafe/FFI boundaries.

## Required real-device matrix

| Scenario | Expected result | Result |
|---|---|---|
| KDE monitor selection | Native chooser appears; selected monitor produces frames | Not run |
| KDE window selection | Selected window produces frames, if offered | Not run |
| Cancel chooser | Clean cancellation; no active stream/session | Not run |
| Deny permission | Typed permission/cancellation outcome; no leaked session | Not run |
| 1080p30 for 10 minutes | Stable timestamps, bounded memory, no deadlock | Not run |
| Stop during active capture | Stream and session released promptly | Not run |
| Close/revoke portal session | Capture terminates cleanly | Not run |
| Display unplug/reconfigure | Typed error/recovery; no crash | Not run |
| Multi-monitor | Correct dimensions and independent stream identity | Not run |

## Evidence template

For every run, record:

- Date/time and Framelet commit SHA
- Arch/CachyOS release, kernel, KDE Plasma version
- `xdg-desktop-portal`, `xdg-desktop-portal-kde`, PipeWire, WirePlumber versions
- GPU and driver (including whether hardware acceleration was active)
- Test case, exact commands, terminal output, pass/fail, and observed failure
- Approximate CPU and peak memory for sustained capture

A test is **passed** only after it has been run on the named real desktop and its evidence has been recorded. Otherwise its status remains **Not run**.
