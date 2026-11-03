# ADR-0006: Start Linux screen-capture PoC with lamco-wayland portal integration

- **Status:** Accepted for evaluation only; dependency not approved for production
- **Date:** 2026-11-03

## Context
Wayland is a first-class platform and must use compositor-mediated source selection and permissions. The existing candidate evaluation names lamco-wayland but has not tested the current release line or portal lifecycle. Current upstream documentation identifies the 0.8.x line and portal session API; X11 is explicitly not supported by this library.

## Decision
Start with an isolated, opt-in Linux Wayland PoC using the lamco-wayland portal feature, pinned to an exact version in a standalone PoC manifest. The first milestone validates portal manager initialization, compositor permission/source selection, session creation, stream metadata, cancellation, and resource release. The next milestone will test PipeWire frame delivery and conversion to Framelet-owned frames.

The PoC is not part of the production workspace and is not wired into the desktop application. It must not run automatically in CI because it triggers an OS permission dialog. A separate compile-only CI check may validate that the pinned crate builds.

## Alternatives
- Start with direct PipeWire capture: rejected because it would bypass the portal consent flow.
- Implement X11 first: rejected; it does not address the first-class Wayland target.
- Adopt lamco-wayland for production immediately: rejected until dependency/unsafe audit, real compositor tests, and frame delivery validation pass.

## Consequences
A current portal path can be tested without prematurely coupling core to it. The PoC requires a real Wayland desktop and cannot prove device capture in a headless CI runner. The library remains a candidate only.

## Validation
Record exact crate version, features, native requirements, license, unsafe/FFI review, GNOME/KDE results, permission denial/cancel behavior, stream formats, frame delivery, stop/release behavior, and memory/CPU.
