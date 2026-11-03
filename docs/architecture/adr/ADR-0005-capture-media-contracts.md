# ADR-0005: Owned media blocks and explicit clock contracts

- **Status:** Accepted for PoC; exact formats and transport remain open
- **Date:** 2026-11-03

## Context
Native capture backends expose different pixel/audio formats, clock domains, lifecycles, and buffer ownership. Letting backend handles or borrowed memory leak into core/media code would couple the architecture to a particular backend and create lifetime hazards.

## Decision
The application boundary uses owned, validated media blocks and backend-neutral metadata. Video frames carry dimensions, pixel format, per-plane stride/data, monotonic timestamp, and sequence number. Audio blocks carry sample rate, channel layout, sample format, frame count, and source-clock timestamp/sample position. Source clocks are mapped onto one recording timeline by the media pipeline. Backend handles and borrowed buffers never cross the adapter boundary.

Queues are bounded and control events are separate from media payloads. Video may drop stale frames with metrics; audio loss must be surfaced as degraded/failed output. Exact queue capacities, pixel formats, and ring-buffer implementation are determined by PoC measurements.

## Alternatives
- Expose backend frame/stream types: rejected due to dependency leakage and incompatible ownership.
- Force zero-copy in the first API: rejected; safe ownership is more important and copy cost must be measured first.
- Use callback arrival wall-clock time: rejected; it is not a reliable shared A/V clock.

## Consequences
The API is portable and testable, but initial adapters may copy media data. Format negotiation and clock mapping need explicit tests. No lock-free unsafe implementation is permitted.

## Validation
Contract tests validate frame layout, checked arithmetic, timestamp monotonicity/discontinuities, queue bounds, audio overrun, and cancellation. Real-device PoCs measure copy cost, latency, memory, and drift.
