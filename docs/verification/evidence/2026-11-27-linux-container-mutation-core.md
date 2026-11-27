# Evidence record: mutation testing of framelet-core

- **Requirement IDs:** none directly; supports test-strength criterion A-9
- **Roadmap criterion:** A-9
- **Date:** 2026-11-27
- **Tester:** coding agent (Claude Code), awaiting maintainer review
- **Commit SHA / build profile:** working tree on top of `6205b0c0bfe51c94703abc648bd5eaef96896161` containing the tests added with this record (uncommitted at run time; re-run after merge for the merge commit) / debug (cargo-mutants default test profile)
- **Environment:** Linux cloud container, Ubuntu 24.04.5 LTS, kernel 6.18.44, 4 vCPUs; no desktop/compositor needed
- **Hardware:** container vCPUs; no GPU used
- **Devices:** none (core tests use fake ports only)
- **Runtime dependencies:** rustc 1.100, cargo-mutants 27.1.0 (`cargo install --locked cargo-mutants@27.1.0`)

## Procedure

1. `cargo install --locked cargo-mutants@27.1.0`
2. Configuration `.cargo/mutants.toml` excludes `**/*_tests.rs` and `crates/core/src/contract/**` (test-only fakes and contract suites, ADR-0021/ADR-0022).
3. `cargo mutants -p framelet-core -j 2`

## Expected result

Zero missed mutants, or every missed mutant justified as equivalent below.

## Observed result

First run (before new tests): 256 mutants; 134 caught, 26 missed, 94 unviable, 2 timeouts. The misses were untested `Display`/`Debug` impls, accessors (`RecordingId::value`, `includes_microphone`, `as_nanos`, `duration_nanos`, `StorageSessionId::as_str`), `RecordingSession::resume`, `recv_timeout` predicate, and a timeline comparison. Tests were added in `domain.rs`, `ports.rs`, `timeline_tests.rs`, `data_plane_tests.rs` and `application_tests.rs` (exact messages, round-trip values, resume transition, immediate `recv_timeout` with a queued item, sink drop visible to the monitor, first sample at a closed pause start).

Final run: **256 mutants tested in 5 m: 159 caught, 1 missed, 94 unviable, 2 timeouts.**

- Unviable (94): mutants that do not compile (for example `Default::default()` for types without `Default`); not test outcomes.
- Timeouts (2): `data_plane.rs:242 Shared<T>::end with ()` and `data_plane.rs:373 <impl Drop for VideoSink>::drop with ()`. With the stream end never recorded, consumer tests waiting for `Received::Ended` block until cargo-mutants' timeout; the test suite does not pass, so the mutants are detected. `dropping_a_video_sink_is_visible_to_its_monitor` fails on them as well, but the run is aborted by the hanging tests first.

### Justified survivor

| Mutant | Why it is equivalent |
|---|---|
| `timeline.rs:282:37` replace `<` with `<=` in `RecordingTimeline::active_elapsed` (`let covered_end = if ts < segment.end { ts } else { segment.end };`) | The two branches differ only when `ts == segment.end`, and then both yield the same value (`ts` and `segment.end` are equal). No observable behaviour can change. `inside` is computed on the next line independently of this expression. |

## Artifacts

`mutants.out/` is not committed (`.gitignore`); CI job `mutants` uploads it as artifact `mutants-out`.

## Verdict

Pass on the uncommitted working tree: zero unjustified surviving mutants. This record is not the proof for ticking A-9: that requires ADR-0022 acceptance and a recorded mutation run on a committed SHA (the `mutants` CI full run on `main`).
