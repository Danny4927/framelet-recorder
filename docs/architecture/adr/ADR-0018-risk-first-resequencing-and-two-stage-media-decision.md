# ADR-0018: Risk-first re-sequencing and a two-stage media backend decision

- **Date:** 27 November 2026
- **Status:** Accepted — planning and decision process
- **Amends:** ADR-0015 decisions 2 and 3 (selection gates); ADR-0004 remains in force until a stage 2 ADR
- **Related:** ADR-0004, ADR-0013, ADR-0015, ADR-0017

## Context

The previous roadmap made the microphone, screen, and camera phases depend on the media backend evaluation, while ADR-0015 required, before any selection, evidence that needs real capture adapters and clean-machine packaging: A/V drift under real capture, CPU/RSS measurements, independent playback on every OS, and full plugin licence review. The dependencies were circular, so the deferral had no end. At the same time, three risks that could invalidate scope had no prototype: Windows Graphics Capture without first-party `unsafe`, webcam support, and the redistributability of H.264/AAC encoders. After 26 PRs the application could not record.

## Decision

1. **Re-sequence the roadmap** into phases A–H: core consolidation (A); time-boxed risk spikes with Go/No-Go ADRs (B); a walking skeleton that records microphone audio to an explicitly interim WAV format (C); a provisional media backend (D); screen (E), webcam (F), composition (G); hardening and release (H).
2. **Two-stage media decision.**
   - *Stage 1 — provisional selection (Phase D).* Allowed once the B3 encoder/licence matrix exists and the candidate passes: synthetic end-to-end recording in the target format with automated output inspection, encoder/sink/timeout/interruption failure tests, a bounded-queue stress test, and a clean-machine install smoke test on Windows and Linux. The backend may then be used **only** inside one media adapter crate, enforced by `scripts/check-architecture.py`.
   - *Stage 2 — production approval (Phase H).* Requires the full ADR-0015 gate list: playback in two independent players per OS, measured performance against SPECIFICATION.md §8, sustained-load and long-run sync evidence, exact runtime/plugin inventory with licences, and reproducible packaging.
3. **Every exit criterion needs proof.** Criteria are proven by named tests, CI jobs, or evidence records as defined in `docs/verification/README.md`; completeness is tracked in `docs/verification/traceability.md`.
4. The former cross-cutting "Phase 7" becomes a per-slice Definition of Done.

## Alternatives

- Keep deferring until all ADR-0015 gates pass: rejected; circular, and no gate can be reached.
- Select GStreamer for production now: rejected; packaging and licence evidence do not exist.
- Skip the walking skeleton and wait for the target format: rejected; it delays the first end-to-end proof of lifecycle, finalization, and recovery with real data.

## Consequences

- A provisional backend can be replaced by writing another media adapter; the core and other adapters are unaffected (hexagonal boundary).
- The interim WAV output must be labelled as such in the UI and must not be advertised as the MVP format.
- A No-Go in Phase B may change product scope (for example Windows support or the default output format); that change is made explicitly in SPECIFICATION.md rather than discovered late.

## Validation

The roadmap's phase table and dependency column reflect this order; each phase lists criteria with a proof column; the verification framework and traceability matrix exist and are referenced from the roadmap.
