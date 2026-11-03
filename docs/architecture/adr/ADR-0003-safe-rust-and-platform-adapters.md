# ADR-0003: Safe Rust and platform adapters

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
Desktop capture APIs are native and often exposed through FFI. Framelet prioritizes memory safety and a reviewable security boundary over implementing every platform feature at any cost.

## Decision
All first-party Rust crates use #![forbid(unsafe_code)]. OS-specific behavior is isolated in adapter crates using public safe Rust APIs. Framelet will not add raw FFI, unsafe blocks, unsafe trait implementations, pointer casts, or lint exceptions to bridge missing APIs. Third-party dependencies may contain unsafe internals only after review of their unsafe/FFI boundary, maintenance, advisories, license, transitive dependencies, native libraries, and loading behavior.

Use OS-mediated source selection and permissions. If no suitable safe API exists, defer the capability or choose another implementation.

## Alternatives
- Permit narrowly scoped unsafe FFI: rejected because it violates the explicit safety constraint.
- Trust a popular wrapper without review: rejected; popularity does not establish API coverage or supply-chain suitability.
- Implement only features exposed by a safe adapter: accepted fallback.

## Consequences
Positive: no first-party unsafe code and clearer review boundaries. Negative: some native features may be inaccessible; dependency selection is constrained. A dependency containing unsafe code is not described as globally unsafe-free.

## Validation
Every first-party crate opts into forbid(unsafe_code); CI enforces it. Dependency adoption records source/API review, unsafe/FFI findings, RustSec, license, maintenance, platform behavior, and packaging.
