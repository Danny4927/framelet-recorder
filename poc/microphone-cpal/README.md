# CPAL microphone capture PoC

This standalone package probes microphone enumeration and safe F32 input capture. It is outside the production workspace and writes no files.

## Scope

- Uses CPAL 0.18.2, pinned exactly.
- Linux backends are PipeWire and ALSA; CPAL's default host chooses a backend.
- Lists available input devices and selects by the displayed index for a short capture.
- Starts capture only after the explicit `--record` command.
- Accepts only the device's default F32 input format.
- Copies samples into a preallocated, fixed-capacity, single-producer/single-consumer ring.
- Counts overflowed samples instead of blocking or silently overwriting unread samples.
- Stops after ten seconds by default; no audio files or persistent data are written.

## Build and run

Install native Linux development packages for both backends (for Arch/CachyOS, install `pipewire` and `alsa-lib`; for Debian/Ubuntu, install `libpipewire-0.3-dev` and `libasound2-dev`).

From the repository root:

```bash
cargo check --manifest-path poc/microphone-cpal/Cargo.toml
cargo test --manifest-path poc/microphone-cpal/Cargo.toml
cargo clippy --manifest-path poc/microphone-cpal/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --list
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --record --device 0 --duration-seconds 10
```

The device index is the index displayed by `--list`. The stream starts only for `--record`. Samples remain in memory and are discarded at shutdown.

## Callback contract

The CPAL callback runs on a dedicated high-priority audio thread. It does only bounded, preallocated sample copies and atomic counter updates. It performs no allocations, locks, sleeps, logging, filesystem access, or channel sends. The ring capacity is two seconds at the selected device's default rate/channel count. On overflow, new samples are dropped and counted; unread audio is not overwritten.

## Limitations / gates

- Only default F32 configurations are supported in this milestone.
- No resampling, conversion, audio file output, pause/resume, or A/V synchronization.
- Device names are displayed only for local selection and are not persisted.
- Real Windows WASAPI and Linux PipeWire/ALSA capture must be verified on devices.
- CPAL has third-party unsafe and native FFI internals; this is not a claim that the dependency graph is unsafe-free.
- RustSec, transitive dependency, license, source-level unsafe/FFI, Windows, and native packaging review remain required before production adoption.
