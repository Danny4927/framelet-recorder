# ADR-0001: Modular monolith and ports-and-adapters

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
Framelet is a single-user desktop recorder without a server component. Native capture, media processing, local storage, and UI must work across Windows and Linux. Direct coupling of use cases to OS APIs or media libraries would make testing and replacement difficult.

## Decision
Use one desktop process as a modular Rust workspace. The application core owns use cases and depends on narrow application-owned ports. Capture, media, and storage adapters implement those ports and are wired by the desktop composition root. Core does not import Slint or concrete platform/media crates. Add crates only when real code justifies a boundary.

## Alternatives
- Single crate with modules: simpler initially, weaker dependency enforcement as integrations grow.
- Microservices/helper daemon: unnecessary deployment, IPC, permissions, and operational complexity.
- UI directly orchestrates backend libraries: couples lifecycle and error handling to presentation.

## Consequences
Positive: replaceable adapters, deterministic core tests, explicit dependency direction. Negative: more crate/API wiring; avoid over-fragmentation. No backend service is introduced.

## Validation
Verify core has no dependency on Slint, OS capture crates, or concrete media runtime. Core tests run without physical devices.
