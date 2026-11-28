# Verification framework

This document defines how Framelet proves that a roadmap phase is **complete** (every requirement it owns is implemented) and **correct** (the implementation behaves as specified, on the platforms claimed). The [implementation roadmap](../architecture/implementation-roadmap.md) references these rules from every phase.

## 1. Requirement traceability (completeness)

- Every requirement from [SPECIFICATION.md](../../SPECIFICATION.md), [SECURITY.md](../../SECURITY.md), and the arc42 quality scenarios gets a stable ID in [traceability.md](traceability.md) (`REQ-<AREA>-<NN>`). IDs are never reused or renumbered.
- Each row is assigned to exactly one roadmap phase and names its verification method (section 2), the automated tests that prove it, and the evidence records it needs.
- A row's state is one of: `open`, `implemented`, `automated-tested`, `ui-integrated`, `real-device-validated`. A row is **verified** when it reaches the state its method requires (for example a real-device row needs `real-device-validated`).
- A phase is complete only when every row assigned to it is verified. Rows moved to another phase need a roadmap update in the same PR.
- Every normative SPECIFICATION.md item carries its ID inline (`[REQ-<AREA>-<NN>]`). Since roadmap criterion A-10, [`scripts/check-traceability.py`](../../scripts/check-traceability.py) (tested by `scripts/test_check_traceability.py`) runs in the Linux and Windows CI jobs and fails on a malformed or duplicate ID, an invalid phase or state, a tagged ID without a matrix row or a row without a tag, an untagged normative item outside the sections the script lists as informative, a test reference that does not resolve to a `#[test] fn` in the named crate, a missing linked evidence file, or a row at `automated-tested` or higher that cites no test, no workflow link, and no CI job or step (`CI job "linux"`, `CI step "<name>"`) present in `.github/workflows/ci.yml`. Whether a row's state is honest is still checked by review.

## 2. Verification methods (correctness)

| Method | Proves | Minimum bar |
|---|---|---|
| Unit / aggregate test | Domain invariants, value-object validation | Asserts the rejected *and* accepted cases |
| Exhaustive or property-based test | Total behaviour over a finite or generated input space (transition table, timeline mapping, buffer arithmetic) | Covers boundaries (zero, maximum, overflow) |
| Application test with fakes | Orchestration order, failure handling, event publication | Asserts call order and that no port is touched on rejected commands |
| Shared port contract suite | Fakes and real adapters behave alike | Same suite runs against the fake and every adapter |
| Fault-injection test | Error paths in adapters | Every fallible external call has at least one injected failure |
| Output inspection | Produced files are well-formed | Container, codec, stream count, duration checked by a tool (for example `ffprobe`) |
| Golden-frame test | Deterministic composition | Pixel-exact or with a stated tolerance |
| Stress / soak test | Bounded resources, no silent loss | Stated duration and thresholds; queue high-water and RSS recorded |
| Mutation testing | Test strength in the core | Surviving mutants killed or justified |
| Real-device evidence record | Behaviour on real OS/hardware | Uses the template in [evidence/TEMPLATE.md](evidence/TEMPLATE.md) |
| Review | Architecture and safety rules not expressible as tests | Reviewer sign-off against AGENTS.md in the PR |

A test that cannot fail when the behaviour is wrong (for example it only checks "no panic") does not count as proof.

## 3. Reference environments

Real-device evidence uses these reference environments. Exact versions and device models are recorded in each evidence record; the first record for an environment fixes its baseline.

| ID | Environment |
|---|---|
| ENV-L1 | Current GNOME on Wayland (Fedora or Ubuntu, current release) |
| ENV-L2 | Current KDE Plasma on Wayland |
| ENV-W10 | Windows 10 22H2 |
| ENV-W11 | Windows 11, current release |
| DEV-MIC-USB / DEV-MIC-INT | One USB microphone, one built-in microphone |
| DEV-CAM-UVC / DEV-CAM-INT | One UVC USB webcam, one built-in camera |

Standard durations: **smoke** 1 minute, **standard** 10 minutes, **soak** 60 minutes (arc42 Q-07).

## 4. Evidence records

- Stored under `docs/verification/evidence/` as `YYYY-MM-DD-<env>-<topic>.md`, using [TEMPLATE.md](evidence/TEMPLATE.md).
- Must name the commit SHA and build profile; evidence for another commit does not transfer automatically. A later PR that touches the verified path either re-runs the procedure or states why the evidence still applies.
- Raw artifacts that are small and contain no captured content (for example `ffprobe` JSON, metric logs) are committed next to the record. Never commit recordings or anything containing captured screen, camera, or audio content.

## 5. Phase closure

A phase is closed by a PR that links evidence for every exit criterion, updates the traceability states, updates the roadmap status table, and carries a reviewer sign-off. A criterion without a link stays unchecked.
