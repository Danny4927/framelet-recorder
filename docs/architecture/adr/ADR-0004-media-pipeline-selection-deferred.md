# ADR-0004: Defer media pipeline selection until proof of concept

- **Status:** Accepted (selection deliberately deferred)
- **Date:** 2026-11-03

## Context
Framelet needs timestamp handling, camera composition, encoding, muxing, pause/resume, and reliable finalization on Windows and Linux. GStreamer Rust bindings are a candidate, but native runtime size, plugin availability, safe API coverage, codec redistribution, and clean-machine packaging have not been validated.

## Decision
Do not approve a production media runtime yet. Define an application-owned media pipeline port and compare GStreamer with viable alternatives in a focused PoC. The PoC must demonstrate:
- video and microphone input (real or synthetic as appropriate);
- common monotonic timeline and pause/resume;
- screen + webcam compositing;
- MP4/H.264 and AAC/M4A output where distributable;
- bounded buffering, overload visibility, and clean finalization;
- safe public Rust integration with no first-party unsafe code;
- Windows and Linux packaging on clean machines;
- license, maintenance, RustSec, dependency, native-library, and plugin-loading review.

Only a new ADR backed by evidence may select the runtime.

## Alternatives
- Select GStreamer immediately: broad functionality, but packaging/plugin/codec assumptions are unverified.
- Build custom encoders/muxer: rejected for MVP scope, correctness, and maintenance.
- Separate OS pipelines: possible fallback, but increases divergence; evaluate only if a shared safe pipeline fails.

## Consequences
Avoids premature commitment, but capture integration cannot be considered complete until PoC passes. GStreamer remains a candidate, not an approved dependency.

## Exit criteria
Publish reproducible results with OS/hardware, exact versions, formats, output inspection, resource measurements, failure tests, plugin inventory, and dependency/security review.
