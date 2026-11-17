# ADR-0010: Make hexagonal architecture and arc42 compliance binding

- **Date:** 17 November 2026
- **Status:** Accepted
- **Supersedes:** None; clarifies and strengthens ADR-0001

## Context

ADR-0001 already selected a modular monolith with ports-and-adapters around OS capture, and the arc42 document describes a composition root and application-owned interfaces. However, the repository's contributor rules only require UI/core/adapters to be separated and OS-specific APIs to sit behind traits. They do not explicitly make hexagonal architecture mandatory across every capability, nor do they define arc42 completeness as an acceptance gate. This leaves room for features to couple UI, core, media, storage, and platform libraries directly.

## Decision

Hexagonal architecture (ports and adapters) is a binding constraint for the entire Framelet application, not just the OS capture boundary.

1. The application core owns use cases, application/domain types, lifecycle orchestration, and port definitions.
2. Inbound adapters (initially Slint UI) translate external commands into inbound-port calls and render state/events; they contain no business rules.
3. Outbound adapters implement core-owned ports for capture, media processing, storage, settings, clocks, and platform capabilities.
4. The desktop executable is the only composition root and injects concrete adapters into core use cases.
5. Dependencies point inward. Core must not depend on concrete UI, OS, capture, media, codec, muxer, or filesystem libraries. Adapters depend on core-owned contracts.
6. Framework/native types and errors are translated at adapter boundaries. No adapter-to-adapter calls, UI-to-adapter shortcuts, or global service locator.
7. Core use cases must be testable with fakes and without physical devices.
8. Structural changes update the relevant arc42 sections in the same PR. Significant decisions receive ADRs.
9. The architecture document must retain all 12 numbered arc42 sections. Completeness and consistency are reviewed for every structural PR.

These constraints apply to new code and refactoring of existing code. A feature is not complete if it bypasses the ports, even if it works end-to-end.

## Alternatives

- Keep ports-and-adapters only around capture: rejected because media, storage, settings, and UI would remain potential coupling paths.
- Allow pragmatic direct calls for early PoCs: rejected for production code; isolated PoCs may use library APIs but must remain outside core and be clearly marked as evaluation-only.
- Treat arc42 as optional documentation: rejected because architecture decisions and implementation status must remain reviewable as the project evolves.

## Consequences

- Clear dependency direction and replaceable adapters across all external capabilities.
- More explicit ports, wiring, and test doubles; avoid speculative ports and empty crates by introducing them when a real use case requires them.
- The desktop composition root becomes responsible for dependency injection.
- Architecture review and arc42 updates are part of definition of done.
- Isolated PoCs remain allowed but cannot be treated as production adapters until they conform to the ports and safety gates.

## Validation

- Inspect the dependency graph: core has no concrete UI/platform/media/storage dependencies.
- Add architecture tests or crate dependency checks as the workspace grows.
- Core use-case tests run using fakes only.
- PR review checks all 12 arc42 sections, verifies no duplicate/missing ADR entries, and checks that implementation status is not overstated.
