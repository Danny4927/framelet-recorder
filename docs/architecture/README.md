# Framelet architecture

This directory is the architecture source of truth. Architecture documentation MUST follow all 12 arc42 sections and Architecture Decision Records (ADRs). The implementation MUST use Domain-Driven Design (DDD) for the application domain and follow hexagonal architecture (ports and adapters); both are binding constraints, not preferences.

- [arc42 architecture](arc42.md)
- [Capture and media contracts](capture-and-media-contracts.md)
- [Implementation roadmap](implementation-roadmap.md)
- [Verification framework](../verification/README.md) and [traceability matrix](../verification/traceability.md)
- [ADR index](adr/README.md)
- [Wayland portal PoC](../../poc/wayland-portal/README.md)

**Baseline:** 3 November 2026. This is an intended architecture, not evidence that native capture or media backends are implemented. Candidate technologies remain unapproved until their PoCs and security reviews pass.

Hexagonal architecture is mandatory across all application capabilities; see [ADR-0010](adr/ADR-0010-binding-hexagonal-architecture-and-arc42.md). DDD is mandatory inside the core; see [ADR-0011](adr/ADR-0011-domain-driven-design.md). The initial bounded context is Recording. All 12 arc42 sections are required. Follow the [implementation roadmap](implementation-roadmap.md) for phase sequencing. Update arc42 when structural decisions change. Add an ADR for decisions that are costly to reverse, affect security/platform compatibility, constrain multiple components, or select significant dependencies. Accepted ADRs are immutable; supersede them with a new record.

| Topic | Status |
|---|---|
| Modular Rust desktop app; no backend service | Accepted |
| Hexagonal architecture across the entire application (not only OS capture) | Binding; see ADR-0010 |
| Domain-Driven Design inside the application core | Binding; see ADR-0011 |
| Recording ubiquitous language and initial bounded context | Binding baseline; evolve with evidence |
| Core owns recording lifecycle and use cases; adapters implement core-owned ports | Binding |
| No first-party unsafe Rust | Binding constraint |
| Slint UI | Existing scaffold; retain provisionally |
| GStreamer, CPAL, nokhwa, PipeWire/Wayland and Windows capture wrappers | Candidates only |
| Frame/audio transport between capture and media | Core-owned bounded sinks (ADR-0020, Accepted); queue sizing still to be measured |
| Storage/media hand-off and shared port contract suites | ADR-0021, Accepted |
| Settings persistence (TOML file adapter `crates/settings`, serde/toml confined to it) | ADR-0023, Proposed |
| arc42 documentation completeness | All 12 sections required; reviewed with every structural change | Binding |


**Status:** The core owns the Recording aggregate and orchestrates its use cases through the granular ports (ADR-0017). The roadmap was re-sequenced risk-first (ADR-0018); see the [implementation roadmap](implementation-roadmap.md) for phases and the [verification framework](../verification/README.md) for how each exit criterion is proven. Capture and media adapters are not yet production-wired.
