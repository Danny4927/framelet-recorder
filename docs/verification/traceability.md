# Requirement traceability matrix

Rules: see the [verification framework](README.md). State values: `open`, `implemented`, `automated-tested`, `ui-integrated`, `real-device-validated`. "Required state" is the state a row must reach to count as verified.

Every normative SPECIFICATION.md item carries its ID inline (for example `[REQ-LIFE-11]`); `scripts/check-traceability.py` (roadmap criterion A-10) checks in CI that every inline ID has exactly one row here, that every row's ID appears in SPECIFICATION.md or SECURITY.md, that phase and state values are valid, that every referenced test exists, that linked evidence files exist, and that rows at `automated-tested` or higher cite a test, a workflow link, or a CI job/step that exists in `.github/workflows/ci.yml` (written `CI job "linux"` or `CI step "<step name>"`). SECURITY.md requirements are not yet tagged inline; until they are, they are traced through the SPECIFICATION.md rows they refine.

Test references use `crate::module::test_name` paths relative to the workspace; the checker resolves them to a `#[test] fn test_name` inside the named crate. Phase values are `Foundation`, `A`–`H`, or `every phase`.

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
| REQ-LIFE-14 | On stop, sources are closed, buffers drained, and the container finalized, in that order (Spec §4.2 step 9) | A | Application test | `framelet_core::application::tests::full_lifecycle_orchestrates_ports_in_order_and_publishes_only_after_finalization` | automated-tested | automated-tested |

## Time and synchronization

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-TIME-01 | Common monotonic origin T0; no wall clock for sync (Spec §6.2) | A | Property / exhaustive | `framelet_core::timeline::tests::exhaustive_single_stream_sequences_up_to_six_ops_match_oracle`, `framelet_core::timeline::tests::exhaustive_multi_stream_sequences_up_to_five_ops_match_oracle`, `framelet_core::timeline::tests::maps_at_u64_max_without_overflow` (origin T0 on `MonotonicInstant`; no wall clock in `crates/core/src/timeline.rs`) | automated-tested | automated-tested |
| REQ-TIME-02 | Paused duration removed from the timeline (Spec §6.2) | A | Property / exhaustive | `framelet_core::timeline::tests::exhaustive_single_stream_sequences_up_to_six_ops_match_oracle`, `framelet_core::timeline::tests::seeded_random_long_sequences_match_oracle`, `framelet_core::timeline::tests::total_paused_equal_to_elapsed_presents_zero`, `framelet_core::timeline::tests::pause_edges_are_half_open_and_zero_length_pauses_are_ignored`, `framelet_core::timeline::tests::open_pause_excludes_later_samples`, `framelet_core::timeline::tests::pause_misuse_is_rejected_with_typed_errors`, `framelet_core::timeline::tests::pause_segment_count_is_capped`, `framelet_core::timeline::tests::resume_before_mapped_paused_sample_is_rejected` | automated-tested | automated-tested |
| REQ-TIME-03 | Timestamp discontinuities and regressions detected (Spec §6.2) | A | Property / exhaustive | `framelet_core::timeline::tests::regression_and_duplicate_are_detected_without_advancing_cursor`, `framelet_core::timeline::tests::discontinuity_threshold_is_exclusive`, `framelet_core::timeline::tests::pause_gap_is_not_a_discontinuity`, `framelet_core::timeline::tests::exhaustive_multi_stream_sequences_up_to_five_ops_match_oracle`, `framelet_core::timeline::tests::seeded_random_long_sequences_match_oracle` | automated-tested | automated-tested |
| REQ-TIME-04 | A/V sync < 100 ms (Spec §8) | E | Real-device evidence | — | real-device-validated | open |
| REQ-TIME-05 | Long-recording sync and bounded memory (Spec §6.2, arc42 Q-07) | G | Soak evidence | — | real-device-validated | open |
| REQ-TIME-06 | Source timestamps preserved where possible (Spec §6.2) | E | Adapter test | — | automated-tested | open |
| REQ-TIME-07 | Variable frame rate and dropped frames handled on the timeline (Spec §6.2) | E | Property test + stress test | — | automated-tested | open |
| REQ-TIME-08 | Audio sample position and timestamps stay consistent (Spec §6.2) | C | Unit + output inspection | — | automated-tested | open |
| REQ-TIME-09 | Clock drift between sources detected (Spec §6.2) | G | Synthetic timeline test + soak evidence | — | real-device-validated | open |

## Backpressure and data plane

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-FLOW-01 | No unbounded frame or audio queue (Spec §5.4) | A | Contract suite | — | automated-tested | open |
| REQ-FLOW-02 | Video drops counted and surfaced (Spec §5.4) | D | Stress test | — | automated-tested | open |
| REQ-FLOW-03 | Audio loss detected and reported (Spec §5.4) | C | Fault-injection test | — | automated-tested | open |
| REQ-FLOW-04 | Audio callback real-time safe (Spec §5.4) | C | Review + test | — | automated-tested | open |
| REQ-FLOW-05 | Audio transferred through a bounded SPSC ring buffer where suitable (Spec §5.4) | C | Review + contract suite | — | automated-tested | open |

## Storage

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-STOR-01 | Never overwrite an existing recording (Spec §7) | Foundation (verified in A) | Adapter test | `framelet_storage::tests::begin_retries_colliding_temporary_names_without_overwriting`, `framelet_storage::tests::abort_removes_only_candidate_and_preserves_existing_recording` | automated-tested | automated-tested |
| REQ-STOR-02 | Exclusive temporary output, publish only after close (Spec §7) | A | Contract suite | — | automated-tested | open |
| REQ-STOR-03 | Incomplete recordings discovered on startup (Spec §7) | C | Adapter test + UI procedure | — | ui-integrated | implemented |
| REQ-STOR-04 | User can select another directory (Spec §7) | C | UI procedure | — | ui-integrated | open |
| REQ-STOR-05 | Disk full / write failure handled (Spec §7, arc42 Q-06) | H | Fault injection on real filesystems | — | real-device-validated | open |
| REQ-STOR-06 | Default output directory is the user's Videos/Framelet directory per platform conventions (Spec §7) | C | Adapter test + UI procedure | — | ui-integrated | open |
| REQ-STOR-07 | Collision-resistant generated filenames (Spec §7) | A | Adapter test | `framelet_storage::tests::begin_retries_colliding_temporary_names_without_overwriting` | automated-tested | automated-tested |
| REQ-STOR-08 | Paths validated and symlinks handled carefully (Spec §7) | C | Adapter test | — | automated-tested | open |
| REQ-STOR-09 | No shell is invoked to create files or directories (Spec §7) | Foundation | Review | — | implemented | implemented |
| REQ-STOR-10 | No arbitrary path fragments accepted from untrusted metadata (Spec §7) | C | Unit + adapter test | — | automated-tested | open |
| REQ-STOR-11 | Output file permissions restricted to the current user where supported (Spec §7) | C | Adapter test per OS | — | automated-tested | open |
| REQ-STOR-12 | Crash/power-loss recovery behaviour documented and tested (Spec §7) | C | Fault injection + documentation | — | automated-tested | open |

## Modes and platforms

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-MODE-MIC | Microphone-only recording (Spec §1.1) | C (WAV), D (target format) | Output inspection + real-device evidence | — | real-device-validated | open |
| REQ-MODE-SCREEN | Screen recording (Spec §1.1) | E | Output inspection + real-device evidence | — | real-device-validated | open |
| REQ-MODE-CAM | Webcam recording with preview (Spec §1.1, §4.2) | F | Output inspection + real-device evidence | — | real-device-validated | open |
| REQ-MODE-PIP | Screen + webcam composition (Spec §1.1, §4.4) | G | Golden-frame + real-device evidence | — | real-device-validated | open |
| REQ-PLAT-WAYLAND | Wayland via portal + PipeWire (Spec §2.2) | E | Real-device evidence ENV-L1, ENV-L2 | — | real-device-validated | open |
| REQ-PLAT-WIN | Windows 10/11 (Spec §2.1) | B, E | Spike + real-device evidence ENV-W10, ENV-W11 | — | real-device-validated | open |
| REQ-PLAT-X11 | X11 tested or explicitly unsupported (Spec §2.2, §12) | H | Decision record | — | implemented | open |
| REQ-PLAT-MACOS | No macOS code, dependencies, CI targets, packaging, or support claims (Spec §2.3) | every phase | Review | — | implemented | implemented |

## Safety, security, and privacy

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-SAFE-01 | No first-party unsafe (Spec §1.3 #5, AGENTS.md) | every phase | `scripts/check-architecture.py` | CI step "Enforce Rust safety and hexagonal architecture" | automated-tested | automated-tested |
| REQ-SAFE-02 | Core depends only on contract crates (Spec §2.4, AGENTS.md) | every phase | `scripts/check-architecture.py` | CI step "Enforce Rust safety and hexagonal architecture" | automated-tested | automated-tested |
| REQ-SEC-01 | Offline operation, no network (Spec §1.2, arc42 Q-08) | H | Network observation evidence | — | real-device-validated | open |
| REQ-SEC-02 | Dependency, licence, and native plugin review (Spec §3.1) | B, D, H | Review records | — | implemented | open |
| REQ-SAFE-03 | First-party application logic written in Rust (Spec §1.3 #4) | every phase | Review | — | implemented | implemented |
| REQ-SAFE-04 | OS permission and privacy mechanisms respected, never bypassed (Spec §1.3 #6, §2.1, §4.2 step 4) | E, F | Real-device evidence | — | real-device-validated | open |
| REQ-SAFE-06 | No panics in production paths; no `unwrap()`/`expect()` on fallible runtime operations (Spec §9) | every phase | Review + Clippy | CI job | automated-tested | open |
| REQ-SEC-03 | No dynamic loading of media plugins from user-writable directories (Spec §6.3) | D | Review + packaging evidence | — | real-device-validated | open |

## Scope

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-SCOPE-01 | No system audio capture (Spec §1.1) | every phase | Review | — | implemented | implemented |
| REQ-SCOPE-02 | Out-of-scope features (cloud, upload, accounts, AI, editing, player, …) are absent (Spec §1.2) | every phase | Review | — | implemented | implemented |

## User experience and devices

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-UX-01 | A recording starts with only a few deliberate interactions (Spec §1) | C | UI procedure | — | ui-integrated | open |
| REQ-UX-02 | Minimal UI (Spec §1.3 #3) | C | UI procedure + review | — | ui-integrated | open |
| REQ-UX-03 | Main window offers mode choices, microphone and camera selection, camera preview, destination, Start, minimal quality and bubble settings (Spec §4.1) | C, F, G | UI procedure + visual regression | — | ui-integrated | open |
| REQ-UX-04 | No advanced codec or pipeline options in the primary view (Spec §4.1) | C | Visual regression + review | — | ui-integrated | open |
| REQ-UX-05 | User chooses a mode before recording (Spec §4.2 step 1) | C | UI procedure | — | ui-integrated | open |
| REQ-UX-06 | Optional three-second countdown before capture (Spec §4.2 step 7) | C | Application test + UI procedure | — | ui-integrated | open |
| REQ-UX-07 | Start, Pause, Resume, Stop controls with recording status and elapsed time (Spec §4.2 step 8, §4.3) | C | UI procedure | — | ui-integrated | open |
| REQ-UX-08 | UI stays responsive and its event loop never blocks (Spec §4.2, §5.4) | C | UI procedure + measurement | — | ui-integrated | open |
| REQ-UX-09 | Spec §4.5 defaults (resolution, frame rate, sample rate, countdown, container, codecs, bubble corner) apply where the device and backend support them (Spec §4.5) | C, D, F, G | Application test + output inspection | — | automated-tested | open |
| REQ-DEV-01 | Available devices and supported formats are enumerated (Spec §4.2 step 2) | C, E, F | Adapter contract suite + real-device evidence | — | real-device-validated | open |
| REQ-DEV-02 | Device-supported formats take precedence; no claimed setting the device cannot provide (Spec §4.5) | F | Application test | — | automated-tested | open |
| REQ-CAP-01 | Capture source selected through the OS-provided picker where applicable (Spec §4.2 step 3) | E | Real-device evidence | — | real-device-validated | open |

## Picture-in-picture bubble

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-PIP-01 | Default bubble position bottom-right (Spec §4.4) | G | Golden-frame test | — | automated-tested | open |
| REQ-PIP-02 | Default bubble size about 20% of output width (Spec §4.4) | G | Golden-frame test | — | automated-tested | open |
| REQ-PIP-03 | Camera aspect ratio preserved with a deterministic fit/crop policy (Spec §4.4, §6.4) | G | Golden-frame test | — | automated-tested | open |
| REQ-PIP-04 | Bubble size configurable: small, medium, large (Spec §4.4) | G | Golden-frame test + UI procedure | — | ui-integrated | open |
| REQ-PIP-05 | Bubble corner configurable (Spec §4.4) | G | Golden-frame test + UI procedure | — | ui-integrated | open |
| REQ-PIP-06 | Preview mirroring never mirrors the screen or the output (Spec §4.4) | G | Golden-frame test | — | automated-tested | open |
| REQ-PIP-07 | GPU compositing only through safe, reviewed, portable APIs (Spec §6.4) | G | Review | — | implemented | open |

## Architecture, capture contracts, and media

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-ARCH-01 | No backend or services (Spec §5) | every phase | Review | — | implemented | implemented |
| REQ-ARCH-02 | Workspace members are buildable; no broken placeholders (Spec §5) | every phase | CI build | CI job "linux", CI job "windows" | automated-tested | automated-tested |
| REQ-ARCH-03 | UI code performs no capture, encoding, disk I/O, or blocking enumeration (Spec §5.1) | C | Review + `scripts/check-architecture.py` | — | automated-tested | open |
| REQ-ARCH-04 | Capture, media processing, and storage run outside the UI thread (Spec §5.4) | C | Review + UI procedure | — | ui-integrated | open |
| REQ-ARCH-05 | Module responsibilities follow Spec §5.1 (Spec §5.1) | every phase | `scripts/check-architecture.py` + review | — | implemented | open |
| REQ-CAP-02 | Narrow, safe capture traits; frames have clear ownership (Spec §5.2) | A | Contract suite | — | automated-tested | open |
| REQ-CAP-03 | Frame metadata carries size, stride, pixel format, monotonic timestamp, sequence number, owned data (Spec §5.2) | A | Unit | `framelet_capture_api::tests::video_frame_accepts_valid_owned_buffer`, `framelet_capture_api::tests::video_frame_rejects_zero_dimensions`, `framelet_capture_api::tests::video_frame_rejects_short_stride`, `framelet_capture_api::tests::video_frame_rejects_short_buffer` | automated-tested | automated-tested |
| REQ-CAP-04 | Audio blocks carry sample rate, channels, sample format, monotonic position, owned samples (Spec §5.2). `AudioBlock` has no sample-format field yet; samples are implicitly `f32`. The explicit field is pending ([audio block contract](../architecture/capture-and-media-contracts.md), ADR-0005) | A | Unit | — | automated-tested | open |
| REQ-CAP-05 | Dimensions, strides, lengths validated with checked arithmetic before allocation (Spec §5.2) | C, E | Property / fuzz test | — | automated-tested | open |
| REQ-ERR-01 | Typed error set covers permission, cancellation, device, capture, encoder, storage, disk space, format, overrun (Spec §5.3) | A | Unit | — | automated-tested | open |
| REQ-MEDIA-01 | Video output MP4/H.264 (Spec §6.3) | D | Output inspection | — | automated-tested | open |
| REQ-MEDIA-02 | Audio output AAC in MP4/M4A (Spec §6.3) | D | Output inspection | — | automated-tested | open |
| REQ-MEDIA-03 | Software encoder fallback; hardware encoder never assumed (Spec §6.3) | D | Integration test + real-device evidence | — | real-device-validated | open |
| REQ-MEDIA-04 | Media pipeline and plugins tested and packaged explicitly per OS (Spec §6.3) | D | Packaging evidence | — | real-device-validated | open |

## Error handling

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-ERR-02 | Errors are actionable for the user (Spec §9) | C | UI procedure | — | ui-integrated | open |
| REQ-ERR-03 | Permission denied explains which OS permission is needed and where (Spec §9) | E | UI procedure + real-device evidence | — | real-device-validated | open |
| REQ-ERR-04 | Cancelled source selection returns to Ready without starting (Spec §9) | E | Adapter contract test | — | automated-tested | open |
| REQ-ERR-05 | Camera disconnect stops safely or continues only when the mode stays valid and the user is informed (Spec §9) | F | Real-device evidence | — | real-device-validated | open |
| REQ-ERR-06 | Microphone disconnect is reported; no falsely complete recording (Spec §9) | C | Fault injection + real-device evidence | — | real-device-validated | open |
| REQ-ERR-07 | Encoder failure stops the pipeline and reports recoverability (Spec §9) | D | Integration test | — | automated-tested | open |

## Performance, process, and acceptance

| ID | Requirement (source) | Phase | Method | Tests / evidence | Required state | State |
|---|---|---|---|---|---|---|
| REQ-PERF-01 | Spec §8 targets measured with full RSS and published context; misses documented (Spec §1.3 #3, §8) | H | Measurement evidence | — | real-device-validated | open |
| REQ-PROC-01 | UI controls invoke real use cases or are visibly unavailable; each increment ships a manual test procedure (Spec §4.6) | every phase | Review + UI procedure | — | ui-integrated | open |
| REQ-PROC-02 | Planning kept as Markdown in the repository, not GitHub Issues (Spec §4.7) | every phase | Review | — | implemented | implemented |
| REQ-TEST-01 | Unit tests cover the Spec §10.1 areas (Spec §10.1) | every phase | Unit | — | automated-tested | open |
| REQ-TEST-02 | Integration scenarios of Spec §10.2 run on every supported OS (Spec §10.2) | H | Integration tests + real-device evidence | — | real-device-validated | open |
| REQ-ACC-01 | MVP acceptance criteria met (Spec §10.3) | H | Release review with linked evidence | — | real-device-validated | open |
