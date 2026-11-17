# ADR-0012: Define granular Recording bounded-context ports

- **Date:** 17 November 2026
- **Status:** Accepted
- **Related:** ADR-0001, ADR-0002, ADR-0005, ADR-0009, ADR-0010, ADR-0011

## Context

The first Phase 1 application slice used one broad resource facade. That is sufficient for a fake implementation, but it does not express independent ownership, lifecycle, cancellation, and failure boundaries for screen, camera, microphone, media processing, and storage. Platform adapters must remain replaceable and must not depend on one another.

## Decision

The Recording bounded context owns separate outbound contracts for screen capture, camera capture, microphone capture, media pipeline, storage, monotonic time, settings, session metadata, event publication, and metrics snapshots.

1. Ports use owned, backend-neutral values. Existing `AudioBlock` and `VideoFrame` contracts are reused; native handles and backend errors never cross a port.
2. Capture adapters own device handles and OS permission/session resources. Start/stop methods execute on workers; callbacks remain bounded and non-blocking.
3. The media pipeline accepts owned blocks, preserves source timestamps/order, and has explicit pause/resume/drain/close operations.
4. Storage owns temporary files, flush/close, atomic publication where supported, and abort cleanup. A completed artifact is not visible before publication succeeds.
5. The session repository persists domain metadata only. Media payload durability and crash recovery belong to the storage phase.
6. The monotonic clock is the only source for elapsed time and ordering; wall-clock time is not used for media synchronization.
7. Settings own serialization and platform-specific locations in their adapter.
8. Events represent domain facts after transitions; publication must not block real-time callbacks. Metrics are snapshots and high-rate values must be coalesced.
9. Port errors are typed and translated at adapter boundaries. Each contract documents ownership, thread expectations, cancellation, and shutdown behavior.
10. The broad `RecordingResources` facade remains a compatibility seam for the initial application service. It is not permission for adapters to call one another; the desktop composition root wires granular adapters.

## Alternatives

- One generic infrastructure port: rejected because it obscures ownership and failure handling.
- Expose CPAL, PipeWire, OS capture, codec, or filesystem types in core: rejected because it reverses the dependency rule.
- Persist media buffers in the session repository: rejected because session metadata and large media payloads have different lifecycle and durability requirements.
- Publish metrics directly from device callbacks: rejected because callbacks must remain real-time safe.

## Consequences

- Adapters can be implemented and tested independently against stable contracts.
- The desktop composition root is responsible for assembling adapters.
- The initial application service can continue using a fakeable facade until adapter wiring is delivered.
- Durable storage implementation, crash recovery, and production source integrations remain later-phase work.

## Validation

- Core depends only on backend-neutral contract crates.
- No core port exposes native backend types.
- Port contract tests use fakes and do not require physical devices.
- arc42 sections 4–6 and the implementation roadmap describe the current boundaries.
