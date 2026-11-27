# ADR-0017: Orchestrate recording use cases through the granular ports

- **Date:** 27 November 2026
- **Status:** Accepted
- **Supersedes:** decision 10 of ADR-0012 (the `RecordingResources` compatibility facade)
- **Related:** ADR-0002, ADR-0010, ADR-0011, ADR-0012, ADR-0014, ADR-0018

## Context

ADR-0012 defined granular outbound ports but kept a broad `RecordingResources` facade as a "compatibility seam" for the application service. In practice the use cases only ever talked to that facade, whose methods returned `Result<_, String>`, and `ApplicationError::Port(String)` carried the message. The granular ports were used by the storage adapter but by no use case. The project therefore had two parallel contracts, stringly-typed errors contrary to AGENTS.md, and no tested statement of the orchestration order or failure policy. Every real adapter written against that state would have had to be re-wired.

## Decision

1. `RecordingApplication` receives a `RecordingPorts` value from the composition root and coordinates the `RecordingSession` aggregate only through `RecordingStoragePort`, `MediaPipelinePort`, `ScreenCapturePort`, `CameraCapturePort`, `MicrophoneCapturePort`, and `RecordingEventPublisher`. The facade is removed.
2. Capture ports are optional in `RecordingPorts`. A configuration that needs a missing capability is rejected with `ApplicationError::CapabilityUnavailable` before any port is called. This lets each vertical slice ship only what is implemented, without fake adapters.
3. The sources selected for a recording must match the configuration exactly. This is a domain invariant (`RecordingConfiguration::requires_source`, `validate_source_selection`, `DomainError::MissingSource`/`UnexpectedSource`).
4. Failures are typed as `CapabilityFailure { capability: RecordingCapability, error: PortError }`. `ApplicationError::CapabilityFailed` carries the decisive failure plus every further failure observed while stopping or releasing resources; `CleanupIncomplete` reports a cancel that took effect but could not release everything. Nothing is swallowed.
5. Control-plane order:
   - **prepare:** validate sources and capabilities → `storage.begin` → `media.prepare` (on failure: `storage.abort`; no session is stored) → Ready.
   - **start:** `media.start` → capture start in the fixed order screen, camera, microphone (on any failure: stop already started sources, `media.close`, `storage.abort`) → Recording. A failed start makes the session Failed.
   - **pause/resume:** `media.pause`/`media.resume`; a port failure leaves the state unchanged.
   - **stop:** Finalizing → stop all captures → `media.drain` → `media.close` → `storage.publish` → Completed. Every step is attempted even if an earlier one failed, so accepted media is flushed as far as possible. On any failure the session is Failed, nothing is published, and the temporary candidate is **kept** for recovery discovery rather than aborted (SPECIFICATION.md §9: preserve recoverable data).
   - **cancel:** only before capture starts; `media.close` → `storage.abort` → Cancelled.
6. Lifecycle events (`Ready`, `Started`, `Paused`, `Resumed`, `Completed`, `Failed`, `Cancelled`) are published after the corresponding transition.
7. Adapter handles (`MediaSessionId`, `CaptureSessionId`) are held by the application service next to the aggregate, not inside it: they are infrastructure references, not domain state.

The core module is split into `domain` (aggregate, invariants), `ports`, and `application` (use cases). Public paths re-exported from the crate root are unchanged for domain types.

## Out of scope (tracked in roadmap Phase A)

Routing of media payloads from capture to pipeline (data plane), the storage-to-media hand-off of the output target, the recording timeline model, and the use of the session repository, settings, metrics, and clock ports.

## Alternatives

- Keep the facade and add typed errors to it: rejected; it would preserve two contracts and hide per-capability failure ownership.
- Make the application generic over each port type: rejected for now; six type parameters add noise without a measured benefit. Boxed trait objects of core-owned traits keep the dependency direction intact.
- Abort storage on every stop failure: rejected; it would delete recoverable data.

## Consequences

- Adapters are written against one contract; fakes in `application_tests.rs` exercise every failure path without hardware.
- `RecordingUseCases` now requires only `Send` (the ports are `Send`, not `Sync`); callers drive it from one worker.
- The desktop composition root will construct `RecordingPorts` when the first slice (roadmap Phase C) is wired.

## Validation

`crates/core/src/application_tests.rs` asserts call order for the full lifecycle and every mode, each failure path's rollback and reporting, that illegal commands reach no port, and that unavailable capabilities and mismatched sources are rejected before any port call. `crates/core/src/domain.rs` tests the transition table exhaustively. Traceability rows REQ-LIFE-01 to REQ-LIFE-13 reference these tests.
