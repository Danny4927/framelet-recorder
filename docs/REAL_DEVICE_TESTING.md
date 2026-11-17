# Real-device testing — Linux Wayland / KDE Plasma

## Scope and honesty

A CI compile or unit test is not a real desktop capture test. These tests require a logged-in Wayland desktop, a working D-Bus user session, the KDE portal backend, PipeWire, and user interaction with the native source chooser. CI cannot claim they passed unless it runs that actual environment.

## Target environment

- Arch Linux or CachyOS, fully updated.
- KDE Plasma session running on Wayland (not X11/XWayland).
- `xdg-desktop-portal` and `xdg-desktop-portal-kde`.
- PipeWire and a working session manager (normally WirePlumber).
- Rust 1.92+.

Check the session and services:

```bash
printf 'session=%s desktop=%s wayland=%s\\n' "$XDG_SESSION_TYPE" "$XDG_CURRENT_DESKTOP" "$WAYLAND_DISPLAY"
systemctl --user --no-pager status xdg-desktop-portal.service
systemctl --user --no-pager status pipewire.service
systemctl --user --no-pager status wireplumber.service
```

Expected: `XDG_SESSION_TYPE=wayland`, a KDE desktop identifier, and non-empty `WAYLAND_DISPLAY`. Services may be socket/D-Bus activated.

## Milestone 3 — sustained PipeWire capture

Run from the repository root:

```bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo test --manifest-path poc/wayland-portal/Cargo.toml
cargo clippy --manifest-path poc/wayland-portal/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start --duration-seconds 600
```

The ten-minute default-duration run requests a monitor/window via the OS portal, receives frames and prints a progress report every five seconds. It does not save a recording.

## Required real-device matrix

| Scenario | Expected result | Result |
|---|---|---|
| KDE monitor selection | Native chooser appears; frames arrive and validate | Not run |
| KDE window selection | Selected window produces validated frames, if offered | Not run |
| GNOME selection | Portal selection and validated frames | Not run |
| Cancel chooser | Clean cancellation; no active stream/session | Not run |
| Deny permission | Clean error; no active stream/session | Not run |
| 120-frame smoke run | Reaches frame target and shuts down | Not run |
| Ten-minute run | Periodic diagnostics; no timeout/deadlock; bounded RSS | Not run |
| PipeWire service restart/closure | Process exits cleanly and releases session | Not run |
| Portal session closure/revocation | Capture stops; no orphan worker | Not run |
| Display unplug/reconfigure | Failure is visible; no crash or FD leak | Not run |
| Multi-monitor | Correct dimensions and independent stream identity | Not run |

## Evidence template

For each run, record:

- Date/time and Framelet commit SHA.
- Distribution/release, kernel, KDE Plasma or GNOME version.
- `xdg-desktop-portal`, portal backend, PipeWire and WirePlumber versions.
- GPU/driver.
- Command, selected source type, terminal output, pass/fail and observed failure.
- Reported receive FPS, sequence gaps, timestamp regressions, corrupted/rejected frames.
- CPU and peak process RSS measured externally (for example with `/usr/bin/time -v` or system monitor).
- Whether shutdown completed and the process exited.

A test is **passed** only after execution on the named real desktop and evidence is recorded. Otherwise leave its status **Not run**.
