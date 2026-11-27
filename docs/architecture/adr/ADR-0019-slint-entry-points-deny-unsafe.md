# ADR-0019: Slint-generated entry points use `deny(unsafe_code)` instead of `forbid`

- **Date:** 27 November 2026
- **Status:** Accepted — records an existing exception
- **Related:** ADR-0003, `docs/architecture/quality-gates.md`

## Context

AGENTS.md requires `#![forbid(unsafe_code)]` in every first-party crate and forbids weakening it. The desktop crate's `src/main.rs` and its visual regression test include Slint-generated code via `slint::include_modules!()`. The generated code locally allows `unsafe_code` for its internals; `forbid` cannot be overridden by a local `allow`, so these two compilation roots fail to compile with `forbid`. They have used `#![deny(unsafe_code)]` since the Slint shell was introduced, and `scripts/check-architecture.py` already enforces exactly that, but the exception was documented only in the quality-gates text, not as a decision.

## Decision

1. Only `apps/desktop/src/main.rs` and `apps/desktop/tests/visual_regression.rs` may use `#![deny(unsafe_code)]`. Every other first-party compilation root keeps `forbid`.
2. No handwritten `unsafe` and no handwritten `allow(unsafe_code)` is permitted in those files or in modules they declare; the only `allow` comes from Slint-generated code.
3. `scripts/check-architecture.py` keeps the exact allowlist; adding a path requires a new ADR.
4. First-party UI logic that grows beyond composition should move into modules or crates that use `forbid`.

## Alternatives

- Keep the exception undocumented: rejected; it contradicts AGENTS.md as written.
- Drop Slint: out of scope; Slint is the retained UI candidate.

## Consequences

The safety claim for first-party code remains: no handwritten unsafe. Generated Slint internals are third-party code and are reviewed as part of the Slint dependency.

## Validation

`scripts/check-architecture.py` requires `deny` for exactly these two roots and `forbid` everywhere else; a grep for `allow(unsafe_code)` in first-party sources finds no handwritten occurrence.
