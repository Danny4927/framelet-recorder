# ADR-0009: Add a bounded worker-level audio block transport

- **Date:** 17 November 2026
- **Status:** Accepted for backend-neutral API; no production capture backend approved

## Context

The capture API represents owned interleaved F32 audio blocks, while the isolated CPAL PoC uses a preallocated SPSC sample ring to keep its callback bounded. A backend-neutral handoff contract between consumer and media workers is missing. A general-purpose queue in the callback would violate real-time constraints.

## Decision

Add `AudioBlockQueue` to `framelet-capture-api` as a fixed-capacity FIFO for worker-to-worker transfer of owned blocks. Its sample rate and channel count are fixed at construction; it enforces a maximum interleaved sample count per block, rejects non-finite samples, and rejects new blocks when full without discarding already accepted audio. It exposes a rejection counter.

The queue may allocate and is prohibited from callback context. A capture adapter keeps its preallocated SPSC sample ring, drains it on a consumer worker, and only then creates/publishes owned AudioBlocks.

## Alternatives

- Unbounded channel: rejected because memory could grow without limit.
- Drop oldest audio blocks: rejected because it loses accepted audio.
- Reuse callback ring as application transport: rejected because it couples core/pipeline to adapter-specific callback storage.
- Add a queue dependency: deferred; safe standard-library collections suffice for this single-owner worker boundary.

## Consequences

This is an in-memory contract only. CPAL remains outside the production workspace, and no device support is claimed. Queue sizing must be measured. Queue ownership must remain on a worker; do not put a blocking mutex around it in a device callback.

## Validation

Unit tests cover FIFO ordering, configured capacity, rejected-block accounting, format mismatch, maximum block size, and finite samples. Production adoption remains gated on Linux PipeWire/ALSA and Windows WASAPI device tests, dependency review, timestamp validation, and an end-to-end recording pipeline.
