# Architecture Decision Records

ADRs record decisions that materially shape Framelet. Accepted records are immutable; supersede them with a new ADR.

| ID | Title | Status |
|---|---|---|
| [ADR-0001](ADR-0001-modular-monolith-and-ports-adapters.md) | Modular monolith and ports-and-adapters | Accepted |
| [ADR-0002](ADR-0002-core-owned-recording-lifecycle.md) | Core-owned recording lifecycle | Accepted |
| [ADR-0003](ADR-0003-safe-rust-and-platform-adapters.md) | Safe Rust and platform adapters | Accepted |
| [ADR-0004](ADR-0004-media-pipeline-selection-deferred.md) | Defer media pipeline selection | Accepted (defer) |
| [ADR-0005](ADR-0005-capture-media-contracts.md) | Owned media blocks and clock contracts | Accepted for PoC |
| [ADR-0006](ADR-0006-wayland-portal-poc.md) | Wayland portal PoC using lamco-wayland | Evaluation only |
| [ADR-0007](ADR-0007-lamco-pipewire-frame-delivery.md) | Do not use the lamco-pipewire 0.8.4 manager frame receiver | Accepted for PoC |
| [ADR-0008](ADR-0008-cpal-microphone-capture.md) | Evaluate CPAL for microphone capture | Evaluation only |
| [ADR-0009](ADR-0009-bounded-audio-block-transport.md) | Bounded worker-level audio block transport | Accepted for backend-neutral API |
| [ADR-0010](ADR-0010-binding-hexagonal-architecture-and-arc42.md) | Make hexagonal architecture and arc42 compliance binding | Accepted |
| [ADR-0011](ADR-0011-domain-driven-design.md) | Make Domain-Driven Design binding for the application domain | Accepted |
| [ADR-0012](ADR-0012-recording-core-port-boundaries.md) | Define granular Recording bounded-context ports | Accepted; decisions 3 and 4 amended by ADR-0020/ADR-0021 once those are accepted |
| [ADR-0013](ADR-0013-media-pipeline-evaluation.md) | Evaluate GStreamer through an isolated media pipeline PoC | Proposed — evaluation only |
| [ADR-0014](ADR-0014-transactional-filesystem-storage.md) | Transactional filesystem recording storage | Accepted for the Phase 3 adapter slice; hand-off and abort semantics amended by ADR-0021 once accepted |
| [ADR-0015](ADR-0015-media-backend-evaluation-decision.md) | Complete media pipeline evaluation; keep production backend deferred | Accepted — evaluation decision only |
| [ADR-0016](ADR-0016-native-windows-title-bar.md) | Native dark Windows title bar | Accepted for the Windows desktop shell |
| [ADR-0017](ADR-0017-port-based-use-case-orchestration.md) | Orchestrate recording use cases through the granular ports | Accepted; supersedes ADR-0012 decision 10; call order amended by ADR-0020/ADR-0021 once accepted |
| [ADR-0018](ADR-0018-risk-first-resequencing-and-two-stage-media-decision.md) | Risk-first re-sequencing and a two-stage media backend decision | Accepted — planning/decision process |
| [ADR-0019](ADR-0019-slint-entry-points-deny-unsafe.md) | Slint-generated entry points use `deny(unsafe_code)` | Accepted — records existing exception |
| [ADR-0020](ADR-0020-bounded-data-plane-and-source-loss.md) | Core-owned bounded data plane, overload policy, and source loss | Proposed — implemented, awaiting human acceptance |
| [ADR-0021](ADR-0021-storage-media-hand-off-and-contract-suites.md) | Storage-to-media hand-off through the core, and shared port contract suites | Proposed — implemented, awaiting human acceptance |
| [ADR-0022](ADR-0022-cargo-mutants-mutation-testing.md) | Mutation testing of `framelet-core` with cargo-mutants | Proposed — implemented, awaiting human acceptance |

Use the next sequential identifier. Each record states context, decision, alternatives, consequences, and validation.
