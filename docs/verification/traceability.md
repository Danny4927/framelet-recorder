# Requirement traceability matrix

Rules: see the [verification framework](README.md). State values: `open`, `implemented`, `automated-tested`, `ui-integrated`, `real-device-validated`. "Required state" is the state a row must reach to count as verified.

This initial matrix covers the core lifecycle requirements in full and seeds IDs for the remaining SPECIFICATION.md areas. Completing the ID assignment for every SPECIFICATION.md and SECURITY.md requirement is roadmap criterion A-10.

Test references use `crate::module::test_name` paths relative to the workspace.

## Core lifecycle and orchestration

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-LIFE-01 | Only the core owns the state machine; illegal transitions rejected (Spec §5.3) | A | Exhaustive | `framelet_core::domain::tests::transition_table_is_exhaustively_enforced` | automated-tested | automated-tested |
| REQ-LIFE-02 | Illegal commands do not touch external resources (arc42 Q-11) | A | Application test | `framelet_core::application::tests::illegal_commands_never_reach_a_port` | automated-tested | automated-tested |
| REQ-LIFE-03 | Result path is shown only after successful finalization (Spec §4.2 step 10) | A | Application test | `framelet_core::application::tests::full_lifecycle_orchestrates_ports_in_order_and_publishes_only_after_finalization`, `framelet_core::domain::tests::artifact_is_exposed_only_after_completion_and_cleared_on_failure` | automated-tested | automated-tested |
| REQ-LIFE-04 | Finalization failure never reports success (Spec §1.3 #8, arc42 Q-10) | A | Application test | `framelet_core::application::tests::publication_failure_fails_session_without_artifact_and_keeps_candidate_for_recovery`, `framelet_core::application::tests::drain_failure_still_closes_pipeline_and_never_publishes`, `framelet_core::application::tests::capture_stop_failure_is_reported_and_prevents_success` | automated-tested | automated-tested |
| REQ-LIFE-05 | Failed stop preserves recoverable data (Spec §9 "Disk full") | A | Application test | `framelet_core::application::tests::publication_failure_fails_session_without_artifact_and_keeps_candidate_for_recovery` | automated-tested | automated-tested |
| REQ-LIFE-06 | Shutdown/cancellation releases all capture resources (Spec §5.4) | A | Application test | `framelet_core::application::tests::capture_start_failure_rolls_back_started_sources_and_reservations`, `framelet_core::application::tests::cancel_releases_prepared_resources_and_blocks_start` | automated-tested | automated-tested |
| REQ-LIFE-07 | Errors are visible, never silent (Spec §9) | A | Application test | `framelet_core::application::tests::rollback_failures_are_reported_not_swallowed`, `framelet_core::application::tests::cancel_with_cleanup_failure_cancels_but_reports_failure` | automated-tested | automated-tested |
| REQ-LIFE-08 | Microphone-only mode cannot add a second microphone (Spec §1.1) | A | Unit | `framelet_core::domain::tests::microphone_only_cannot_add_optional_microphone` | automated-tested | automated-tested |
| REQ-LIFE-09 | Each mode records exactly its sources; optional microphone where specified (Spec §1.1, §6.1) | A | Unit + application test | `framelet_core::domain::tests::required_sources_follow_mode_and_optional_microphone`, `framelet_core::domain::tests::source_selection_must_match_exactly`, `framelet_core::application::tests::each_mode_starts_exactly_its_required_sources`, `framelet_core::application::tests::source_selection_must_match_configuration_before_any_port_is_called` | automated-tested | automated-tested |
| REQ-LIFE-10 | Unavailable capabilities are reported, not faked (Spec §4.6) | A | Application test | `framelet_core::application::tests::unavailable_capability_is_rejected_before_any_port_is_called` | automated-tested | automated-tested |
| REQ-LIFE-11 | Stop is idempotent (Spec §4.3) | A | Application test | — | automated-tested | open |
| REQ-LIFE-12 | Cancel/discard of an active recording with explicit warning (Spec §4.3) | C | Application test + UI procedure | — | ui-integrated | open |
| REQ-LIFE-13 | Capture never starts before explicit Start (Spec §4.2) | A | Application test | `framelet_core::application::tests::full_lifecycle_orchestrates_ports_in_order_and_publishes_only_after_finalization` (no capture call before `start`) | automated-tested | automated-tested |

## Time and synchronization

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-TIME-01 | Common monotonic origin T0; no wall clock for sync (Spec §6.2) | A | Property / exhaustive | — | automated-tested | open |
| REQ-TIME-02 | Paused duration removed from the timeline (Spec §6.2) | A | Property / exhaustive | — | automated-tested | open |
| REQ-TIME-03 | Timestamp discontinuities and regressions detected (Spec §6.2) | A | Property / exhaustive | — | automated-tested | open |
| REQ-TIME-04 | A/V sync < 100 ms (Spec §8) | E | Real-device evidence | — | real-device-validated | open |
| REQ-TIME-05 | Long-recording sync and bounded memory (Spec §6.2, arc42 Q-07) | G | Soak evidence | — | real-device-validated | open |

## Backpressure and data plane

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-FLOW-01 | No unbounded frame or audio queue (Spec §5.4) | A | Contract suite | — | automated-tested | open |
| REQ-FLOW-02 | Video drops counted and surfaced (Spec §5.4) | D | Stress test | — | automated-tested | open |
| REQ-FLOW-03 | Audio loss detected and reported (Spec §5.4) | C | Fault-injection test | — | automated-tested | open |
| REQ-FLOW-04 | Audio callback real-time safe (Spec §5.4) | C | Review + test | — | automated-tested | open |

## Storage

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-STOR-01 | Never overwrite an existing recording (Spec §7) | Foundation (verified in A) | Adapter test | `framelet_storage::tests::begin_retries_colliding_temporary_names_without_overwriting`, `framelet_storage::tests::abort_removes_only_candidate_and_preserves_existing_recording` | automated-tested | automated-tested |
| REQ-STOR-02 | Exclusive temporary output, publish only after close (Spec §7) | A | Contract suite | — | automated-tested | open |
| REQ-STOR-03 | Incomplete recordings discovered on startup (Spec §7) | C | Adapter test + UI procedure | — | ui-integrated | implemented |
| REQ-STOR-04 | User can select another directory (Spec §7) | C | UI procedure | — | ui-integrated | open |
| REQ-STOR-05 | Disk full / write failure handled (Spec §7, arc42 Q-06) | H | Fault injection on real filesystems | — | real-device-validated | open |

## Modes and platforms

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-MODE-MIC | Microphone-only recording (Spec §1.1) | C (WAV), D (target format) | Output inspection + real-device evidence | — | real-device-validated | open |
| REQ-MODE-SCREEN | Screen recording (Spec §1.1) | E | Output inspection + real-device evidence | — | real-device-validated | open |
| REQ-MODE-CAM | Webcam recording with preview (Spec §1.1, §4.2) | F | Output inspection + real-device evidence | — | real-device-validated | open |
| REQ-MODE-PIP | Screen + webcam composition (Spec §1.1, §4.4) | G | Golden-frame + real-device evidence | — | real-device-validated | open |
| REQ-PLAT-WAYLAND | Wayland via portal + PipeWire (Spec §2.2) | E | Real-device evidence ENV-L1, ENV-L2 | — | real-device-validated | open |
| REQ-PLAT-WIN | Windows 10/11 (Spec §2.1) | B, E | Spike + real-device evidence ENV-W10, ENV-W11 | — | real-device-validated | open |
| REQ-PLAT-X11 | X11 tested or explicitly unsupported (Spec §12) | H | Decision record | — | implemented | open |

## Safety, security, and privacy

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-SAFE-01 | No first-party unsafe (AGENTS.md) | every phase | `scripts/check-architecture.py` | CI job | automated-tested | automated-tested |
| REQ-SAFE-02 | Core depends only on contract crates (AGENTS.md) | every phase | `scripts/check-architecture.py` | CI job | automated-tested | automated-tested |
| REQ-SEC-01 | Offline operation, no network (Spec §1.2, arc42 Q-08) | H | Network observation evidence | — | real-device-validated | open |
| REQ-SEC-02 | Dependency, licence, and native plugin review (Spec §3.1) | B, D, H | Review records | — | implemented | open |
