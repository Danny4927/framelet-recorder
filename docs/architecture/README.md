# Framelet architecture

This directory is the architecture source of truth. It uses the arc42 structure and Architecture Decision Records (ADRs).

- [arc42 architecture](arc42.md)
- [Capture and media contracts](capture-and-media-contracts.md)
- [ADR index](adr/README.md)
- [Wayland portal PoC](../../poc/wayland-portal/README.md)

**Baseline:** 3 November 2026. This is an intended architecture, not evidence that native capture or media backends are implemented. Candidate technologies remain unapproved until their PoCs and security reviews pass.

Update arc42 when structural decisions change. Add an ADR for decisions that are costly to reverse, affect security/platform compatibility, constrain multiple components, or select significant dependencies. Accepted ADRs are immutable; supersede them with a new record.

| Topic | Status |
|---|---|
| Modular Rust desktop app; no backend service | Accepted |
| Ports-and-adapters boundary around OS capture | Accepted |
| Core owns recording lifecycle | Accepted |
| No first-party unsafe Rust | Binding constraint |
| Slint UI | Existing scaffold; retain provisionally |
| GStreamer, CPAL, nokhwa, PipeWire/Wayland and Windows capture wrappers | Candidates only |
| Exact frame/audio transport and queue sizing | Open; measure in PoC |
