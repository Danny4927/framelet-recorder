# Quality gates

This document defines the automated quality checks required for every Framelet change.
The CI workflow is the executable implementation; this document explains the intent
and the checks that deliberately remain informational.

## Required pull-request checks

| Area | Check | Enforcement |
|---|---|---|
| Rust safety | Workspace lint `unsafe_code = "deny"` and crate-root safety declarations in every first-party crate; Slint macro entry points use `deny` so generated code can locally allow its internals | Blocking |
| Architecture | Core dependency allowlist and forbidden adapter/runtime references | Blocking |
| Documentation | All 12 arc42 sections and repository-local Markdown links | Blocking |
| Style | `cargo fmt --all -- --check` | Blocking |
| Static analysis | Clippy for workspace targets/features and isolated PoCs with warnings denied | Blocking |
| Correctness | Workspace and PoC unit/integration/doc tests | Blocking |
| Platforms | Linux and Windows build, lint and test jobs | Blocking |
| Dependencies | RustSec audit and cargo-deny (licenses, sources, bans and advisories) | Blocking |
| Secrets | Gitleaks repository/history scan | Blocking |

## Informational reports

Workspace coverage is generated with `cargo-llvm-cov` and uploaded as an artifact.
It is intentionally not a merge gate yet: the current codebase is still evolving, and
a global threshold would incentivize low-value tests. Coverage should first establish
a baseline. Future thresholds should focus on domain/application behavior and should
not reward testing trivial getters or platform glue.

Real-device and measurement evidence follows the [verification framework](../verification/README.md).

Visual regression remains a required Linux test because UI regressions are meaningful
for this desktop application. Real-device capture tests remain a separately documented
manual/platform validation activity; CI runners do not prove microphone, camera, portal,
or compositor compatibility on end-user hardware.

## CI execution and caching

- Superseded runs for the same pull request or branch are cancelled; runs on `main` are never cancelled.
- Isolated PoCs run in a separate `poc` matrix job in parallel with the workspace jobs. Each PoC has its
  own Rust build cache, because the PoCs build into their own `target/` directories.
- `cargo clippy --locked` already resolves and type-checks all targets, so no separate `cargo check` step runs.
- `cargo-audit` and `cargo-deny` are installed as prebuilt release binaries via `taiki-e/install-action`
  (already used for `cargo-llvm-cov`) instead of being compiled on every run.

## Architecture enforcement scope

The architecture guard verifies:
- each first-party Rust crate source root denies unsafe Rust; all roots except the Slint macro entry points use `forbid`;
- the core crate has only the approved direct dependency on `framelet-capture-api`;
- core source does not reference known UI, capture, media-runtime or OS adapter APIs.

These checks complement Rust's compiler-enforced module/visibility rules and the
existing crate dependency graph. They do not claim to prove all DDD semantics. Domain
invariants and use-case behavior must still be covered by focused unit and integration
tests, and significant structural decisions must remain documented in arc42 and ADRs.

## Deliberately excluded

- SonarQube/Qodana: no demonstrated incremental value over the Rust toolchain checks
  for this Rust-first repository.
- PMD/Checkstyle: Java-specific.
- cargo-geiger: first-party unsafe is forbidden; auditing unsafe internals in
  third-party crates remains a focused dependency-review task.
- Mutation testing and hard coverage thresholds: defer until the domain behavior and
  coverage baseline are more stable.
- Performance gates: introduce only after repeatable workloads, target hardware and
  regression tolerances have been defined.
