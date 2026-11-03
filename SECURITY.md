# Security Policy

Framelet records potentially sensitive screen, camera, and microphone content. Security and privacy are core product requirements.

## 1. Non-negotiable Rust safety rules

All first-party Rust crates must begin with:

```rust
#![forbid(unsafe_code)]
```

This is mandatory and must not be weakened with crate attributes, module attributes, conditional compilation, or lint allowances.

First-party code must not:
- Use `unsafe` blocks, `unsafe fn`, or `unsafe impl`.
- Use raw-pointer operations or pointer arithmetic.
- Use `transmute`, `get_unchecked`, or unchecked indexing.
- Implement custom FFI bindings.
- Implement `Send` or `Sync` unsafely.
- Use unsafe shortcuts to optimize performance.
- bypass ownership, borrowing, lifetime, or type-system guarantees.
- disable or suppress safety-relevant compiler lints.

Do not use `unwrap()` or `expect()` on runtime-controlled/fallible operations. Handle errors explicitly. Avoid panics in callbacks and worker threads.

## 2. Important limitation: third-party unsafe code

`#![forbid(unsafe_code)]` applies to the crate where it is declared. It does **not** prove that transitive dependencies contain no unsafe code.

Native screen/audio/camera APIs commonly require FFI, COM, C, or OS-specific bindings. Such dependencies may internally use unsafe code. This project does not claim that the entire dependency graph is unsafe-free.

Instead:
- First-party code remains entirely safe Rust.
- Every dependency exposing capture/media functionality receives a documented review.
- Unsafe implementation and FFI boundaries in dependencies are inspected.
- Prefer small, maintained, safe public APIs.
- Reject dependencies with unexplained or excessive unsafe code.
- Re-evaluate dependency versions and advisories on every update.
- If no acceptable safe abstraction exists, do not implement the feature through handwritten unsafe code. Find another dependency/backend or defer the feature.

## 3. Dependency governance

Required checks:
- `cargo audit` against RustSec advisories.
- `cargo deny check` for advisories, licenses, sources, and dependency policy.
- `cargo vet` for supply-chain review where practical.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo fmt --all -- --check`.
- Locked builds using committed `Cargo.lock`.

Dependency rules:
- No wildcard versions.
- No unreviewed Git dependencies.
- No unreviewed build scripts or procedural macros.
- Review transitive dependencies, native libraries, plugins, and redistribution licenses.
- Document exceptions and approvals in the repository.
- Do not add dependencies merely for convenience.

## 4. Capture privacy

- Recording begins only after an explicit user action.
- Use OS-mediated source pickers and permission mechanisms.
- Never bypass Windows, Wayland, or other OS capture restrictions.
- Camera and microphone access must be clearly indicated in the UI.
- Stop must stop and release capture streams.
- No hidden/background recording.
- No network transmission, telemetry, analytics, or crash upload in the MVP.
- Logs must never contain captured image/audio data, transcripts, or sensitive file contents.

## 5. File system security

- Create files exclusively; never overwrite existing recordings.
- Validate output paths and generated filenames.
- Defend against path traversal and symlink attacks.
- Do not execute shell commands using user-controlled strings.
- Use least-privilege file permissions where supported.
- Handle disk-full and partial-write conditions.
- Finalize temporary files only after successful media muxer shutdown.

## 6. Memory and concurrency

- All channels and queues must be bounded.
- Validate dimensions, stride, lengths, and arithmetic before buffer allocation.
- Use checked arithmetic for buffer sizes and offsets.
- No unbounded growth under capture/encoder overload.
- Audio callbacks must not block or perform filesystem/network I/O.
- Handle worker failure and cancellation without panics.
- Do not use unsafe lock-free structures. Use reviewed safe abstractions.

## 7. Build and release

- Pin the Rust toolchain.
- Commit Cargo.lock for applications.
- Build with `--locked`.
- Keep dependencies updated and review changelogs/advisories.
- Produce reproducible or verifiable release artifacts where practical.
- Do not ship arbitrary runtime plugins from untrusted writable locations.

## 8. Reporting a vulnerability

Please do not publish details of an exploitable vulnerability in an issue before maintainers have had a chance to assess it. Use GitHub's private vulnerability reporting for this repository if enabled; otherwise contact the repository owner privately through GitHub.

Include affected version/commit, platform, reproduction steps, and impact. Do not include private recordings or personal data.
