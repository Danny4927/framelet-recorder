# Framelet GStreamer media PoC

**Status: isolated Phase 2 evaluation only. Not a production adapter and not backend approval.**

This PoC is deliberately outside the root Cargo workspace. It accepts Framelet-owned, validated synthetic `VideoFrame` and `AudioBlock` values through GStreamer's Rust `appsrc` API, sets timestamps/durations on media buffers, encodes them with VP8/Vorbis, muxes a Matroska file, and waits for EOS or an error before reporting success.

The current source is an initial pipeline skeleton. Treat CI compilation as API/build evidence only: it is not yet proof that the output is playable, synchronized, performant, packageable, or suitable for production. Phase 2 remains open until independent playback and the measurement/failure matrix below are completed.

## Requirements

- Rust 1.92+
- GStreamer 1.24+ development runtime and plugin packages matching the selected Rust toolchain
- `gst-plugins-base`, `gst-plugins-good`, and the plugins providing `vp8enc`, `vorbisenc`, and `matroskamux`
- Linux packages must provide GStreamer headers/libraries and `pkg-config`
- Windows must use a GStreamer MSI installation whose architecture and MSVC/MinGW toolchain match Rust; ensure the matching `pkg-config` and runtime DLL directories are discoverable

Exact package names differ by distribution. Do not install this PoC's native packages into production builds by implication.

## Run

From the repository root:

```bash
cargo check --manifest-path poc/media-gstreamer/Cargo.toml
cargo test --manifest-path poc/media-gstreamer/Cargo.toml
cargo clippy --manifest-path poc/media-gstreamer/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path poc/media-gstreamer/Cargo.toml -- --check
cargo run --manifest-path poc/media-gstreamer/Cargo.toml -- --output /tmp/framelet-media-poc.mkv
```

The command refuses to overwrite an existing output. It writes only synthetic test patterns and tones; it does not access camera, display, microphone or network devices. A completed file is printed only after EOS is received and the pipeline is set to NULL. On a pipeline error or timeout the candidate output is removed.

## Deliberate limits

- VP8 + Vorbis in Matroska is a permissive evaluation format; this is not yet the intended user-facing MP4/H.264 + AAC format.
- No pause/resume, camera composition, resampling policy, pixel conversion policy beyond GStreamer elements, device integration, hardware encoding, crash recovery, transactional publication, or independent-player validation is claimed.
- The prototype currently feeds synthetic data from a single producer thread. A future measurement pass must add bounded producer queues, separate producer workers, explicit backpressure/drop accounting, and deterministic teardown tests before performance conclusions.
- RSS, CPU, startup, A/V drift, queue high-water marks, and stop-to-EOS measurements are not yet collected.
- Native plugin inventory, codec redistribution terms, CVE/RustSec audit, and clean-machine Windows/Linux packaging remain pending.

## Required evaluation matrix

Record OS release, hardware, runtime version, plugins, Rust version, build type and exact commands for each result.

| Dimension | Required experiment | Evidence |
|---|---|---|
| Output correctness | Open generated output in two independent players; inspect streams and durations | Player/version, stream metadata and result |
| A/V time | Compare generated audio/video start/end and drift under load | Measured drift and timeline notes |
| Backpressure | Slow encoder / constrain queue; verify bounded memory and audio-loss reporting | Queue capacity/high-water, drops/overruns |
| Finalization | EOS, encoder error, sink error, timeout and interrupted process | Result and surviving-file behavior |
| Performance | Startup, CPU, process RSS, throughput, stop-to-EOS | Hardware, OS, duration, metrics |
| Portability | Clean Linux and Windows machine; no development-path assumptions | Install/package inventory |
| Governance | Runtime/plugin license, transitive deps, advisories, maintenance | Reviewed inventory and audit output |

## Architecture guardrails

- This package must stay outside the production workspace until a Phase 2 ADR explicitly approves an implementation path.
- Only `framelet-capture-api` owned values enter the PoC. No GStreamer types may appear in `framelet-core` or any core-owned port.
- First-party source uses `#![forbid(unsafe_code)]`; never add FFI or unsafe glue to make the PoC work.
- Capture, encoding and file I/O stay outside the UI thread and device callbacks.
- The core remains the authority for lifecycle, cancellation and artifact publication.
- No media content is logged and the PoC has no network code.

## Phase 2 validation evidence

No independent-player, bounded-overload, long-run drift, hardware performance, native license/plugin, or clean-machine packaging evidence is recorded by this PoC PR. CI checks compilation, tests, Clippy and formatting only. Do not interpret a green workflow as passing the evaluation matrix above.
