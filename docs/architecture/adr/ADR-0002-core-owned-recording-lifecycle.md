# ADR-0002: Core-owned recording lifecycle

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
Recording spans source selection, permission, startup, buffers, pause/resume, stop, cancellation, and finalization. Multiple authorities can cause races, resource leaks, or false success.

## Decision
The application core is the single authority for application-level recording state and orchestration. UI sends typed commands and renders events. Adapters own device/session resources but report typed events; they do not decide application state transitions. Stop is idempotent. Completion is emitted only after producers stop, accepted data drains, and output finalization succeeds.

## Alternatives
- UI-owned state: rejected because presentation becomes lifecycle authority.
- Distributed application state across adapters: rejected because cross-source coordination is ambiguous.
- Per-source internal state: allowed inside adapters, but does not replace the core lifecycle.

## Consequences
Positive: deterministic transitions and testable use cases. Negative: core needs careful async/event design and must not become a media implementation monolith.

## Validation
Test legal/illegal transitions, permission cancellation, disconnect, pause/resume, idempotent stop, and failed finalization.
