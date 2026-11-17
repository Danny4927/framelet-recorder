# Real-device testing — Framelet capture PoCs

## Scope and honesty

CI compilation and unit tests do not prove real desktop/device capture. Record only tests actually performed on the named OS, backend, and hardware. Do not claim a device test passed because CI compiled the adapter.

## Linux Wayland screen capture environment

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

### Wayland milestone 3 — sustained PipeWire capture

```bash
cargo check --manifest-path poc/wayland-portal/Cargo.toml
cargo test --manifest-path poc/wayland-portal/Cargo.toml
cargo clippy --manifest-path poc/wayland-portal/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path poc/wayland-portal/Cargo.toml -- --start --duration-seconds 600
```

The run opens the OS chooser, receives frames, and prints reports. It does not save a recording.

## Microphone PoC — Linux and Windows

### Linux prerequisites

Install PipeWire and ALSA runtime/development packages. On Arch/CachyOS, install `pipewire` and `alsa-lib`; on Debian/Ubuntu, install `libpipewire-0.3-dev` and `libasound2-dev`.

Try both the PipeWire and ALSA host paths where available. The PoC uses CPAL's default host and a local enumeration index; device names/indices can change between runs.

```bash
cargo check --manifest-path poc/microphone-cpal/Cargo.toml
cargo test --manifest-path poc/microphone-cpal/Cargo.toml
cargo clippy --manifest-path poc/microphone-cpal/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --list
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --record --device 0 --duration-seconds 10
```

### Windows prerequisites

Use Windows 10/11 with a microphone visible in Windows Sound settings and allow microphone access for desktop applications. Build/run the same isolated package using the default CPAL host (WASAPI). No device test is implied by the Linux CI job.

```powershell
cargo check --manifest-path poc/microphone-cpal/Cargo.toml
cargo test --manifest-path poc/microphone-cpal/Cargo.toml
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --list
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --record --device 0 --duration-seconds 10
```

The first microphone milestone accepts only the default F32 input format, keeps data in a fixed two-second ring, reports received sample frames and overrun count, and discards samples on exit. It does not create an audio file.

## Desktop storage/recovery UI smoke test (Linux and Windows)\n\nBuild and run the application, confirm the default output directory is displayed, and press **Temporäre Dateien prüfen**. Create a test file matching `.framelet-pending-test.part` in that directory and scan again; the reported count should increase. Verify the file remains unchanged. The folder change action currently reports that native folder selection is not implemented. Recording preparation remains disabled because no production media backend has been selected. Record real-device results separately; hosted CI is not a device test.\n\n```bash\ncargo run -p framelet-desktop\n```\n\n## Required real-device matrix

| Scenario | Expected result | Result |
|---|---|---|
| KDE monitor selection | Native chooser appears; frames arrive and validate | Not run |
| KDE window selection | Selected window produces validated frames, if offered | Not run |
| GNOME selection | Portal selection and validated frames | Not run |
| Cancel chooser | Clean cancellation; no active stream/session | Not run |
| Deny permission | Clean error; no active stream/session | Not run |
| 120-frame screen smoke run | Reaches frame target and shuts down | Not run |
| Ten-minute screen run | Periodic diagnostics; no timeout/deadlock; bounded RSS | Not run |
| PipeWire service restart/closure | Process exits cleanly and releases session | Not run |
| Linux PipeWire microphone | F32 input is received; clean stream stop | Not run |
| Linux ALSA microphone | F32 input is received; clean stream stop | Not run |
| Windows WASAPI microphone | F32 input is received; clean stream stop | Not run |
| Windows 11 native title bar | Framelet dark caption, light title text, native system buttons and resize behavior | Not run |
| Microphone permission denied | Useful error; no continuing stream | Not run |
| Microphone device unplugged | Stream error observed; clean stop, no panic/leak | Not run |
| Ring-buffer overrun | Drop count increases; callback does not block or panic | Not run |
| Display unplug/reconfigure | Failure is visible; no crash or FD leak | Not run |

## Evidence template

For each run, record:

- Date/time and Framelet commit SHA.
- Distribution/release, OS build, kernel, desktop environment.
- Portal backend, PipeWire, WirePlumber, ALSA, or Windows audio stack version as applicable.
- Microphone model and input mode; do not publish private device identifiers.
- Command, selected local index, sample rate/channels/format, duration, output count, and drop count.
- CPU and peak process RSS measured externally (for example with `/usr/bin/time -v` or system monitor).
- Callback error, device disconnect behavior, and whether shutdown completed.

A test is **passed** only after execution on the named real device and evidence is recorded. Otherwise leave its status **Not run**.
