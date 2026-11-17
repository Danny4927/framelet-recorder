# ADR-0011: Make Domain-Driven Design binding for the application domain

- **Date:** 17 November 2026
- **Status:** Accepted
- **Related:** ADR-0001, ADR-0002, ADR-0010

## Context

Framelet's architecture mandates hexagonal architecture across the whole application. That defines dependency direction and isolates external systems, but does not by itself ensure that business concepts, lifecycle invariants, and domain language are modeled consistently. Without an explicit domain modeling policy, business rules can drift into UI handlers, capture adapters, persistence code, or generic application services.

## Decision

Domain-Driven Design (DDD) is a binding modeling approach for Framelet's application domain. It complements, rather than replaces, hexagonal architecture.

1. The domain model lives inside the application core and is independent of UI, frameworks, persistence, capture libraries, codecs, operating systems, and infrastructure.
2. Maintain a ubiquitous language shared by code, tests, UI terminology, and architecture documentation. Prefer precise domain terms over generic technical names.
3. Model entities, value objects, aggregates, domain services, and domain events where they express real identity, invariants, behavior, or domain facts. Do not add patterns ceremonially.
4. Aggregate boundaries are determined by consistency and invariants. Aggregate roots are the only mutation entry points for their aggregate state.
5. Application use cases coordinate domain behavior and core-owned ports. They must not duplicate the domain model or move invariants into orchestration code.
6. Adapters translate native, framework, and persisted representations at the boundary. Persistence models are not domain models; adapter code does not own business rules.
7. Test domain invariants directly and application use cases with fake ports.
8. The initial MVP is one bounded context, **Recording**. Introduce additional bounded contexts only when distinct models, invariants, or ownership boundaries justify them. Do not introduce microservices or separate deployables as a consequence of DDD.
9. Update the ubiquitous language, arc42, and relevant ADRs in the same PR when domain concepts, invariants, or context boundaries change.

The recording session is the initial aggregate candidate because it owns the recording lifecycle. Phase 1 must validate its exact boundary against concrete use cases. Device handles, media buffers, and infrastructure resources do not automatically belong inside this aggregate.

## Alternatives

- Anemic domain model with all behavior in application services: rejected because lifecycle invariants would be scattered and difficult to protect.
- Put business rules in UI or capture/storage adapters: rejected because those are delivery/infrastructure concerns and cannot provide a consistent domain authority.
- Create many bounded contexts or repositories up front: rejected because no evidence currently justifies additional models or abstractions.
- Apply every DDD pattern mechanically: rejected; use only patterns that clarify actual domain behavior.

## Consequences

- Domain invariants have a clear owner and can be tested independently of devices and native runtimes.
- The team must maintain precise terminology and make modeling decisions explicit.
- Mapping between domain and infrastructure representations is deliberate.
- The initial model stays small; additional contexts and abstractions require evidence.

## Validation

- Domain code has no concrete framework, UI, OS, media, capture, or persistence dependencies.
- Lifecycle and configuration invariants are enforced by domain types/aggregate behavior rather than UI or adapters.
- Domain tests cover invariants; application tests use fake ports.
- arc42 describes the current ubiquitous language, domain boundaries, and any bounded-context relationships.
