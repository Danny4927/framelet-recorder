# ADR-0015: Complete the media pipeline evaluation without selecting a production backend

- **Date:** 17 November 2026
- **Status:** Accepted — evaluation decision; production backend deferred
- **Related:** ADR-0004, ADR-0005, ADR-0009, ADR-0010, ADR-0011, ADR-0012, ADR-0013

## Context

Phase 2 requires a backend comparison and evidence-driven decision, but the existing GStreamer PoC is diagnostic and the product output format, capture topology, packaging model, and target hardware have not been validated. Selecting a backend based only on a successful synthetic pipeline would violate the project's safe-Rust, local-first, portability, bounded-resource, and no-false-success constraints.

The current PoC has been extended with a real bounded producer-to-pipeline-worker channel, explicit overload accounting, audio-gap failure behavior, video drop-newest behavior, pipeline-wide EOS, filesink failure and EOS-timeout integration tests. These checks are still not a substitute for independent-player playback, hardware measurements, interrupted-process recovery, or clean-machine Windows packaging.

## Decision

1. Keep GStreamer as the leading **evaluation candidate**, not a production dependency.
2. Keep production selection deferred under ADR-0004. The production RecordingMediaPort remains backend-neutral; no GStreamer or FFmpeg types enter core or application-owned ports.
3. Require the remaining external validation gates in the implementation roadmap before selection: independent playback on each claimed OS, target-format validation, encoder/interruption failure tests, sustained overload and A/V drift tests, measured CPU/RSS/startup/throughput/finalization, native plugin/license inventory, and clean-machine packaging.
4. Treat Matroska + VP8/Vorbis as a diagnostic test format only. It does not approve MP4 + H.264/AAC or any redistribution model.
5. Keep all media producer queues bounded. Video overload may drop newest/stale frames only with counters; audio overload is an explicit discontinuity/failure, never silent loss. Queue operations on capture callbacks remain non-blocking; the pipeline worker may block while receiving.

## Candidate comparison

| Candidate | Strengths | Costs and unresolved risks | Phase 2 outcome |
|---|---|---|---|
| GStreamer + gstreamer-rs | Safe, generated Rust API; mature graph model; appsrc/appsink and broad codec/container/plugin ecosystem; Linux and Windows installation paths documented | Native runtime and plugin deployment; plugin discovery/security; plugin-specific licensing; version/toolchain matching; plugin availability differs by install profile; package size and clean-machine behavior unmeasured | Leading evaluation candidate; no production approval |
| FFmpeg + Rust bindings (ffmpeg-next / ffmpeg-sys family) | Broad codecs, containers and filters; established cross-platform FFmpeg runtime; Rust wrapper API | Native FFmpeg libraries and ABI/build configuration; wrapper still sits over C FFI/native libraries; LGPL/GPL obligations depend on exact FFmpeg build and enabled components; Windows packaging and safe-boundary audit required | Viable alternative; no PoC evidence yet |
| Pure-Rust media stack | Potentially simpler static/local deployment and fewer system runtime dependencies | No reviewed end-to-end stack has yet demonstrated this project's real-time dual-source capture, encoding, muxing, bounded backpressure, cross-platform playback, and output requirements together; component-level codec/container support is not end-to-end recorder evidence | Revisit after a focused prototype; not selected |
| Hand-written codecs/muxer | Maximum control | High correctness, maintenance, and security risk | Rejected |

### Evidence and source notes

- The gstreamer-rs documentation describes its Rust API as safe and generated from GObject introspection, and documents the native GStreamer and plugin prerequisites for Linux and Windows. The Rust bindings are dual MIT/Apache-2.0; GStreamer itself is LGPL-2.1-or-later. This does not settle the license of every selected plugin or codec: https://gstreamer.freedesktop.org/documentation/rust/stable/latest/docs/gstreamer/index.html
- FFmpeg Rust wrappers remain bindings to native FFmpeg libraries; the exact FFmpeg configuration determines the licensing and redistribution review. A currently published wrapper reference is https://docs.rs/ffmpeg-next/latest/ffmpeg_next/
- Pure-Rust codec/container crates and newer media projects demonstrate promising individual capabilities, but component support alone does not demonstrate the required recorder pipeline. They must pass the same integration and portability gates before becoming a serious alternative.

## Consequences

- The production application remains free of native media runtime dependencies until a superseding ADR approves one.
- GStreamer remains isolated under poc/media-gstreamer; the PoC does not become an adapter by implication.
- Backend-neutral contracts and the core's lifecycle/finalization authority remain unchanged.
- Phase 2 implementation is substantially complete, but the phase remains **in progress** until external playback, hardware, packaging, and licensing evidence is attached to the repository.
- A future selection ADR must include exact runtime/plugin versions, codec/container profiles, license notices and obligations, deployment layout, supported OS matrix, benchmark context, and adapter boundary.

## Validation gates still required

1. Play representative recordings in at least two independent players on every OS claimed.
2. Validate final output format separately from diagnostic Matroska/VP8/Vorbis.
3. Inject encoder-specific and sink failures; verify timeout, interruption, and incomplete-output cleanup.
4. Stress bounded queues for sustained overload, explicit audio discontinuities, video drops, and control-path independence.
5. Record startup, CPU, RSS, throughput, queue high-water marks, A/V drift, and stop-to-finalize latency on documented hardware.
6. Review exact native runtime/plugin inventory, licenses, advisories, and clean-machine Linux/Windows packaging.
