# ADR-0008: Evaluate CPAL for microphone capture

- **Date:** 17 November 2026
- **Status:** Evaluation only — isolated PoC; not approved for production

## Context

Framelet's MVP includes microphone-only recording and optional microphone audio for the other recording modes. The capture API already defines owned interleaved F32 audio blocks, but no microphone backend has been exercised. The project forbids first-party unsafe Rust and requires audio callbacks to be bounded, non-blocking, and free from filesystem/network I/O, mutex waits, and expensive allocation.

## Decision

Evaluate CPAL 0.18.2 in a standalone package at `poc/microphone-cpal`, outside the production workspace.

The PoC is intentionally narrow:

1. Enumerate input devices only when the user invokes `--list`.
2. Start a native input stream only after explicit `--record --device INDEX`.
3. Accept only the device's default F32 configuration for this milestone.
4. Copy callback samples into a preallocated two-second SPSC ring buffer.
5. Count overflowed samples; never block the callback or overwrite unread samples.
6. Stop after a bounded duration (ten seconds by default), report sample/drop counts, then discard the in-memory samples.
7. Do not write audio files or add CPAL to the production workspace.

The exact CPAL version is pinned. CPAL is licensed under Apache-2.0 and exposes safe Rust input-stream APIs, but its platform backends include native/FFI and third-party unsafe internals. The stream is explicitly started with `.play()`; stream construction alone is not treated as capture start.

## Alternatives considered

- **Direct platform APIs:** deferred; each would create a distinct Windows/Linux implementation and safety review.
- **Handwritten FFI or unsafe callback/ring buffer:** rejected by Framelet's mandatory safe-Rust rule.
- **Add CPAL directly to the production workspace:** rejected until the isolated PoC, dependency audit, platform testing, and packaging review pass.
- **Keep audio capture deferred:** acceptable fallback if the safe public API or callback behavior fails validation.

## Consequences

- The PoC can prove device enumeration and sample delivery on the tested host without claiming a supported product backend.
- Only F32 default input configurations are covered; other sample formats, resampling, timestamps, A/V sync, pause/resume, and file output remain open.
- The ring buffer uses fixed memory and reports overflow. A downstream consumer must drain it at an appropriate rate; this PoC intentionally does not add an unbounded queue.
- Linux builds require PipeWire development headers for the enabled PipeWire feature. CPAL also includes its ALSA backend as a target-specific dependency; ALSA development headers are installed in CI. Windows WASAPI behavior still requires device testing.
- The dependency graph is not claimed to be unsafe-free. RustSec, transitive dependency, exact source/unsafe boundary, license/redistribution, and native packaging checks remain mandatory gates.

## Validation required before production adoption

- Linux PipeWire and ALSA device enumeration, capture, stop, and hot-unplug tests.
- Windows WASAPI device enumeration, capture, stop, and hot-unplug tests.
- Validate actual F32 sample delivery, timing/sample-position strategy, callback xruns, ring overflow, and bounded RSS. The isolated ring has FIFO/overflow/wraparound unit tests plus a Loom publication-order model.
- Review CPAL 0.18.2 source and all transitive native/unsafe boundaries.
- Run `cargo audit`, `cargo deny check`, and where practical `cargo vet`.
- Generate and commit a standalone lockfile; verify locked build/test/clippy.
- Decide whether a single F32 path is acceptable or safe conversion for common device formats is required.
- Integrate into the application only after the above evidence is recorded.

## References

- [CPAL crate documentation](https://docs.rs/cpal/0.18.2/cpal/)
- [CPAL 0.18.2 release notes](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md)
- [RustSec advisory database](https://github.com/RustSec/advisory-db)
