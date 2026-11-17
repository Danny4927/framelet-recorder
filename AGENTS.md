# Instructions for contributors and coding agents

These rules apply to all work in this repository.

## Product scope

Framelet is a local-first Rust desktop recorder for Windows and Linux.

MVP modes:
1. Screen recording.
2. Microphone-only recording.
3. Webcam recording.
4. Screen + webcam recording.

No cloud, upload, network service, accounts, system audio, AI, editing, collaboration, or macOS support.

Read `SPECIFICATION.md`, `SECURITY.md`, and `docs/architecture/README.md` before implementing features. Keep `docs/architecture/arc42.md` and relevant ADRs aligned with structural changes. Record significant technology choices as ADRs; do not treat PoC candidates as approved dependencies.

## Mandatory Rust safety

- Every first-party Rust crate must declare `#![forbid(unsafe_code)]`.
- Never write or introduce `unsafe`, raw-pointer operations, FFI bindings, unsafe trait implementations, or unsafe performance shortcuts.
- Never weaken `forbid(unsafe_code)`.
- Do not bypass ownership, borrowing, lifetimes, or type checks.
- Prefer safe APIs and explicit ownership.
- Third-party dependencies may contain unsafe internals; review and document them. Do not claim the entire dependency graph is unsafe-free.
- Do not use `unwrap()` or `expect()` for runtime-controlled errors.
- No panics in real-time callbacks.

If a requirement cannot be implemented without first-party unsafe code, stop and explain the blocker. Do not work around the rule.

## Architecture (binding)

Framelet MUST follow hexagonal architecture (ports and adapters) as a non-negotiable design constraint. This applies to every production feature and every implementation phase.

- The application core owns use cases, business rules, recording lifecycle, and application state.
- Define ports as application-owned Rust traits/types at the core boundary. Core may depend on domain types and port abstractions only.
- Implement inbound adapters for UI/commands and outbound adapters for capture devices, OS APIs, media processing, filesystem, and configuration.
- The desktop executable is the sole composition root: it constructs concrete adapters and injects them into core use cases.
- Dependency direction points inward: adapters depend on core-owned contracts; core never imports or names concrete adapters, Slint, OS APIs, or media libraries.
- Do not pass framework/library types across ports. Translate them into application-owned types and typed errors at adapter boundaries.
- Keep orchestration in application services/use cases; keep device, codec, muxer, and filesystem mechanics in adapters. Do not create a generic service layer that bypasses ports.
- Add architecture tests or compile-time crate-boundary checks where practical. Tests for core use cases must run with fakes and without physical devices.
- Every structural change MUST update the corresponding arc42 section and add/update an ADR when the decision is significant. The implementation roadmap in `docs/architecture/implementation-roadmap.md` is the sequencing authority.
- A feature that violates these rules is not complete, even if it works end-to-end.

Keep UI, application core, capture adapters, media pipeline, and storage separated.
- Keep OS-specific APIs behind internal traits.
- UI must never block on capture, encoding, or disk I/O.
- Use bounded channels and explicit cancellation.
- Audio callbacks must be real-time safe.
- No unbounded frame or audio queues.
- Validate all external/device-provided dimensions, formats, lengths, and paths.
- Use monotonic timestamps for A/V synchronization.
- Do not add a backend or network service.

## Domain-driven design (binding)

Framelet MUST use Domain-Driven Design (DDD) for the application domain. DDD complements hexagonal architecture: the domain model lives inside the core, while ports and adapters isolate external systems.

- Establish and maintain a ubiquitous language in domain code and architecture documentation. Use domain terms consistently; avoid generic names such as Manager, Helper, or Data where a domain concept exists.
- Model meaningful domain concepts explicitly as entities, value objects, aggregates, domain services, and domain events where justified. Do not introduce these patterns ceremonially.
- Define aggregate boundaries around invariants and consistency requirements. Aggregate roots are the only mutation entry points for aggregate state; enforce invariants in the domain model, not in UI code or adapters.
- Keep domain logic independent of frameworks, persistence, capture libraries, codecs, OS APIs, and UI. Domain types must not leak infrastructure types.
- Keep application use cases responsible for coordinating aggregates and ports; do not turn application services into a second domain model.
- Treat persistence and device/media integrations as adapters. Map persistence/native representations to domain types at the boundary; never make database or framework models the domain model.
- Use domain events for meaningful business/domain facts, not as a generic replacement for ordinary method calls or transport messages.
- Add tests for domain invariants and aggregate behavior, and use application-level tests with fake ports. Document bounded contexts and context relationships when more than one distinct model emerges.
- Update the ubiquitous language, arc42, and relevant ADRs in the same PR when domain boundaries or core concepts change.

A feature that violates the domain model or puts business invariants in adapters/UI is not complete, even if it works end-to-end.

## Platform policy

- Windows 10/11.
- Linux Wayland is first-class.
- Linux X11 is separately evaluated and must not be claimed as supported until tested.
- macOS is explicitly unsupported. Do not add macOS code or promise macOS support.

## Dependency policy

Before adding or updating a dependency:
1. Explain why it is needed.
2. Check maintenance and release activity.
3. Check license and redistribution terms.
4. Check RustSec advisories.
5. Review direct and transitive dependencies.
6. Inspect unsafe code and native FFI boundaries.
7. Check Windows/Linux behavior and packaging.
8. Document the decision.

No wildcard dependency versions or unreviewed Git dependencies.

## Code quality

- Keep functions and modules focused.
- Use typed errors; avoid stringly typed error flow.
- Document public APIs.
- Add unit and integration tests for behavior changes.
- Add regression tests for bug fixes.
- Use checked arithmetic for buffer sizes and timestamps.
- Avoid unnecessary allocations and frame copies, but never at the expense of safety.
- Do not silently drop audio or hide video frame drops.
- Do not log captured content or sensitive data.

## Required checks

Run the applicable checks before proposing a change:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo audit
cargo deny check
cargo vet
```

If the workspace is not yet initialized or a command is not configured, report that explicitly; do not claim it passed.

## Change discipline

- Keep changes focused and small.
- Do not introduce placeholder code that appears production-ready.
- Do not claim a platform works without testing it on that platform.
- Update specification/security docs when architecture or security policy changes.
- Explain any unverified assumption.
