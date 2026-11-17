# ADR-0013: Evaluate GStreamer through an isolated media pipeline PoC

- **Date:** 17 November 2026
- **Status:** Proposed — evaluation candidate only
- **Related:** ADR-0004, ADR-0005, ADR-0009, ADR-0010, ADR-0011, ADR-0012

## Context

Phase 1 has established the Recording bounded context, a core-owned media pipeline port, owned audio/video contracts, and the mandatory inward dependency rule. The repository's existing Wayland and CPAL PoCs prove source-specific capture experiments only; neither accepts both media types into an encoded/muxed output. Phase 2 therefore needs a source-independent media pipeline evaluation.

Candidate needs include audio/video encoding and muxing, owned buffer ingestion, monotonic timestamps, finalization/error reporting, bounded backpressure, Windows/Linux availability, a safe Rust API, offline use, and distributable codec/plugin combinations. Production selection is intentionally not made by this ADR.

## Decision

Use GStreamer as the **first evaluation candidate only**, in `poc/media-gstreamer`, outside the production Cargo workspace. The spike uses Framelet-owned synthetic `VideoFrame` and `AudioBlock` values through `appsrc`, encodes VP8/Vorbis, and targets Matroska so the first experiment can validate dual-source appsrc wiring and EOS-driven muxer finalization without prematurely committing to H.264/AAC redistribution.

This does not amend or supersede ADR-0004's deferred production selection. No GStreamer dependency is added to `Cargo.toml`, the production lockfile, core, or desktop application.

## Alternatives considered

| Candidate | Strengths | Open risks / reasons not selected yet |
|---|---|---|
| GStreamer Rust bindings | Safe generated Rust API, graph-based multi-stream pipelines, broad platform/hardware integrations | Native runtime and plugins, plugin loading, packaging, safe API edge coverage, codec licensing/redistribution and exact Windows/Linux parity need verification |
| FFmpeg bindings (e.g. ffmpeg-next / rsmpeg family) | Broad codec/container coverage, familiar demux/encode/mux surface, platform availability | Native library distribution, ABI/build variants, binding API safety boundary, licensing/build configuration, Windows packaging |
| Pure-Rust media stacks | Smaller native footprint and potential static packaging; some media transformations/codecs/containers exist | End-to-end encode+mux maturity, quality/performance, supported profiles and portability must be verified against MVP output requirements |
| Hand-written encoders/muxer | Maximum control | Rejected for complexity, correctness risk and maintenance burden |

## Consequences

- GStreamer gets an isolated, repeatable first vertical experiment with no architectural dependency leak.
- The current Matroska/VP8/Vorbis test output is only a diagnostic format; it does not settle the product's final MP4/H.264/AAC choice.
- A successful compile or PoC run is insufficient for selection. Independent playback, A/V sync, real failure tests, resource measurements, license/plugin review and clean-machine Linux/Windows packaging are mandatory.
- A later ADR must record selection or continued deferral, exact supported plugin/codec inventory, distribution strategy, and adapter boundary.

## Validation gates

1. CI builds/tests/lints/formats the isolated PoC on supported Linux and Windows runners where native prerequisites are available.
2. Generated media plays in two independent players and has inspectable audio/video streams.
3. Timestamps and drift are measured over short and long runs under CPU load.
4. Bounded buffering and overload policies are implemented and verified before performance acceptance.
5. EOS, error, timeout, cancellation and process-interruption behavior are tested.
6. Native runtime/plugins, licenses, advisories and clean-machine packaging are reviewed.

**No production media backend is approved by this ADR.**
