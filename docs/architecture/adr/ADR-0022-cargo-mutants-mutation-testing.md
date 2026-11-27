# ADR-0022: Mutation testing of `framelet-core` with cargo-mutants

- **Date:** 27 November 2026
- **Status:** Proposed — implemented for review; a human maintainer must accept it
- **Related:** roadmap criterion A-9; [quality gates](../quality-gates.md); ADR-0021 (contract suites)

## Context

Roadmap criterion A-9 requires evidence that the core's tests are strong enough: a mutation run on `framelet-core` with zero surviving mutants, or each survivor justified. Line coverage (informational `cargo-llvm-cov` job) shows code was executed, not that a wrong result would be detected. AGENTS.md requires a dependency review before any new tool enters the workflow.

## Decision

1. Use **cargo-mutants 27.1.0** as a developer/CI tool. It is installed as a binary (`cargo install --locked cargo-mutants@27.1.0` locally; `taiki-e/install-action` with `cargo-mutants@27.1.0` in CI). It is **not** a dependency of any workspace crate and never enters the build graph or a release artifact.
2. Configuration lives in `.cargo/mutants.toml`. Test files (`*_tests.rs`) and the test-only `crates/core/src/contract/` module (fakes and contract suites, compiled only under `cfg(test)` or the `contract-tests` feature, ADR-0021) are excluded: they are the oracle, not the subject.
3. CI job `mutants` in `.github/workflows/ci.yml`: on pull requests, only mutants in the diff (`--in-diff`) and only when the PR touches `crates/core/`; a full run on push to `main` and on `workflow_dispatch`. `mutants.out/` is uploaded as an artifact and ignored by git.
4. Surviving mutants are either killed by a meaningful test or justified as equivalent in the evidence record; a survivor is never hidden with `#[mutants::skip]` without a justification recorded in that record.

## Dependency review (AGENTS.md policy)

| Item | Finding |
|---|---|
| Need | Mutation tester for stable Rust that works per package without source annotations; whether it is the only practical option was not verified at review time |
| Maintenance | `cargo info cargo-mutants` on 2026-11-27 (crates.io): version 27.1.0, rust-version 1.88, repository github.com/sourcefrog/cargo-mutants. Release frequency and maintainer count not verified at review time |
| Licence | MIT, per `cargo info cargo-mutants` on 2026-11-27 (crates.io metadata); nothing is redistributed with Framelet |
| Advisories | Not verified at review time: `cargo audit` covers the workspace lockfile, not installed tools. The tool is built with its own lockfile (`--locked`) |
| Transitive dependencies / unsafe | Built from crates.io with its own lockfile; not reviewed crate-by-crate because the tool does not ship. Mutated builds run only the project's own code and tests |
| Coverage by deny/vet | Not covered: `cargo deny` and `cargo vet` check the workspace graph only, which does not contain the tool |
| Platforms | Run on Linux CI only; mutation results concern platform-independent core logic |
| Supply-chain exposure | The CI binary comes from the pinned version via `taiki-e/install-action`; the job has `contents: read` only |

## Alternatives

- **No mutation testing, coverage threshold instead:** rejected; coverage does not detect weak assertions.
- **Manual mutations during review (as in ADR-0021):** kept as a complement, but not repeatable or exhaustive.
- **Full run on every PR:** rejected for CI time; `--in-diff` covers changed code, `main` gets the full run.

## Consequences

- A-9 has repeatable evidence; regressions in test strength surface on PRs that touch the core.
- Mutation runs take minutes; the job is separate from the blocking test jobs.

## Validation

- Evidence: [2026-11-27 Linux container mutation run](../../verification/evidence/2026-11-27-linux-container-mutation-core.md).
