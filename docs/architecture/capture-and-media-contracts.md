# Capture and media contracts

**Status:** Architecture contract. The capture-to-media data plane (§4.2) and the storage/media hand-off (§7.1) are implemented in `framelet-core` per ADR-0020 and ADR-0021 (both Proposed, awaiting acceptance); queue capacities and formats remain measurable implementation decisions.

This document makes the architecture's media boundary concrete without binding it to a capture library.

## 1. Boundary rules

- Adapters return owned, validated data. No backend pointer, native handle, borrowed memory, or backend-specific type crosses the boundary.
- A source has one owner and an explicit lifecycle: discovered -> selected -> prepared -> running <-> paused -> stopped/failed. Application lifecycle remains owned by core.
- Device enumeration, format negotiation, start, and stop execute on workers, never the UI thread.
- Capture callbacks do bounded work only. They do not block, perform I/O, wait on a mutex, log synchronously, or run encoding.
- Every media queue has a fixed capacity. Video overload may discard stale frames and increments a counter. Audio overrun is a recording error/degraded event, never silent loss.
- Control and shutdown signals use a separate path from media payloads so a full video queue cannot prevent Stop.
- Stop is acknowledged only after the producer has stopped and all accepted payloads have been handed off or explicitly accounted for.

## 2. Source identity and capabilities

A source descriptor contains an opaque adapter-local SourceId, a human-readable label, source kind (display/window, camera, microphone), and supported format capabilities. Labels are not assumed stable or unique. Never log raw device identifiers by default.

A source preference may be persisted only if the adapter supplies a stable, non-sensitive identity; otherwise it is session-only. Format negotiation is explicit. The selected format is returned as an immutable configuration before capture starts. A backend must not silently substitute a source, resolution, frame rate, sample rate, channel count, or pixel format.

## 3. Video frame contract

A video frame is immutable after publication and owns its bytes.

Required metadata: non-zero width/height; pixel format; one or more planes, each with row stride and owned bytes; monotonic presentation timestamp relative to the source clock; monotonically increasing sequence number; optional source clock identifier/discontinuity marker.

Initial PoC formats should be limited to BGRA8 or RGBA8 packed frames unless the backend naturally exposes another format without expensive conversion. Multi-plane formats (NV12/I420) require explicit per-plane offsets/strides and validation before adoption.

Validation:
1. Reject zero dimensions and dimensions above a configured maximum before allocation.
2. Check row stride against minimum bytes per row.
3. Use checked multiplication/addition for plane sizes and offsets.
4. Verify each plane contains all addressed bytes; reject truncation.
5. Validate pixel format/plane count combination.
6. Never assume width * height * bytes-per-pixel is safe arithmetic.

The first adapter may copy into an owned byte vector. Zero-copy is not a goal until a safe, lifetime-bound owner abstraction is proven and measured.

## 4. Audio block contract

Audio blocks own interleaved samples and include sample rate, channel count/layout, sample format, first-sample monotonic timestamp or sample position on a source clock, number of frames (samples per channel), and discontinuity/overrun marker.

The core normalizes supported sample formats into a pipeline format outside the device callback. The callback may copy into a preallocated bounded ring buffer using a reviewed safe abstraction. It must not allocate a new vector, block on a full queue, or perform conversion/encoding in callback context.

Audio timestamps should be mapped from backend stream clock/sample position to the shared monotonic recording timeline. Callback arrival timestamp alone is not assumed to equal first-sample capture time.

### 4.1 Audio worker transport

`framelet-capture-api` provides `AudioBlockQueue`, a bounded FIFO for owned blocks passed between worker threads. It is deliberately not real-time safe: queue methods may allocate and require exclusive mutable access. Never call it inside an audio device callback.

The queue fixes sample rate and channel count at construction and bounds both block count and maximum interleaved samples per block. It rejects mismatched formats, oversized blocks, and non-finite sample values. When full, it rejects the newest block, increments a rejection counter, and retains accepted blocks. The caller must surface `AudioTransportFull` as an explicit overrun/failure; it must not silently retry or discard accepted audio.

The CPAL PoC's preallocated SPSC ring remains the callback-facing boundary. A consumer worker drains that ring, creates owned `AudioBlock` values, and then publishes them to the worker-level queue.

### 4.2 Data plane: core-owned bounded sinks (ADR-0020)

- `MediaPipelinePort::prepare(id, &configuration, &target)` creates one bounded channel per required source with `data_plane::media_inputs`, keeps the `MediaStreams`, and returns `PreparedMedia { session, sinks }`. The core rejects sinks that do not match the configuration.
- At start the core moves each sink into its capture port: `start(&source, VideoSink)` for screen and camera, `start(&source, AudioSink)` for the microphone. Capture and media adapters never reference each other.
- `try_send` never blocks. Video: a full queue discards the oldest frame and returns `DroppedStale { dropped_total }`. Audio: a full queue rejects the newest block, keeps all accepted audio, returns `Discontinuity { lost_blocks_total }`, and marks `gap_before` on the next accepted `AudioPacket`. Either returns `Closed` once the consumer released its stream.
- Capacity is fixed and validated at construction (ceilings 64 frames / 1024 blocks); queue length never exceeds it. `StreamStats` reports capacity, occupancy, high-water mark, accepted, delivered, dropped, lost audio frames, discontinuity episodes, end, and consumer state.
- Sinks are used on capture workers only; `try_send` takes a short lock and is not callback-safe. Audio callbacks keep the preallocated SPSC ring of §4.1.
- Before `start`, a pipeline does not consume from its streams (contract suites depend on this). `drain` consumes until every stream has ended; producers are stopped first.
- **Trailing audio loss (consumer obligation).** Audio rejected after the last accepted block is carried by no packet. After an audio stream returns `Received::Ended`, the media pipeline MUST read the final `stats()` and treat a non-empty `pending_gap` like a marked `AudioGap` (count it, propagate it to the degraded/failed outcome). Ignoring it would be silent audio loss. Contract suite: `media::audio_trailing_loss_is_reported_after_stream_end`.
- Dropping a sink ends the stream as `Stopped`; `report_source_lost` consumes the sink and ends it as `SourceLost`. Capture `stop` releases the sink before returning and is idempotent; a failed `start` releases it too.
- The core observes each started source through a `StreamMonitor`; `process_source_losses` and `stop` turn a `SourceLost` end into `RecordingEvent::SourceLost` and the mode-dependent outcome (`RecordingConfiguration::source_loss_outcome`).

## 5. Time model

- Each source reports monotonic timestamps and a source clock identity.
- At prepare/start, core chooses a recording timeline origin.
- An adapter reports timestamp discontinuities explicitly; it does not reset timestamps silently.
- Media pipeline maps source clocks to the recording timeline and tracks drift.
- Pause creates an inactive interval excluded from output presentation time. Resume starts a new active segment.
- Wall-clock time is metadata for filenames only, never A/V synchronization.
- Core owns the `RecordingTimeline` aggregate (`crates/core/src/timeline.rs`): origin T0, closed pause segments `[start, end)` (capped at `MAX_PAUSE_SEGMENTS`), at most one open pause, and the last source timestamp per stream (`SourceKind`). `map` yields `Presented(pts)`, `DuringPause`, `BeforeOrigin`, `Regression { previous, current }` (also for duplicates) or `Discontinuity { gap, pts }` when the pause-excluded gap exceeds the configured threshold. A pause may not begin at or before an already mapped sample, so per-stream PTS never decrease; likewise a resume must be strictly after every mapped sample (`ResumeBeforeMappedSample`), so a sample reported `DuringPause` never becomes active retroactively. Cursor rules: `BeforeOrigin` and `Regression` samples do not update the stream's last timestamp; `DuringPause`, `Presented` and `Discontinuity` samples do. The discontinuity gap is the active (pause-excluded) time between this sample and the stream's previous recorded sample, including one recorded `DuringPause` (whose active position is the pause start); a stream's first sample is never flagged. All arithmetic is checked; `TimelineError::Overflow` is a defensive invariant-violation case the invariants rule out.

## 6. Events and errors

Adapter events include source ready, frame/audio available, source paused/resumed/stopped, permission denied, selection cancelled, device lost, unsupported format, overrun/discontinuity, and backend failure.

Errors are typed and preserve an internal cause for diagnostics. User cancellation is not a failure. Permission denial is distinct from cancellation. Device loss is distinct from unsupported format. Error text must not include captured content or unnecessary device identifiers.

## 7. Pipeline contract

The media pipeline accepts negotiated source configurations and owned blocks. It is responsible for clock mapping, optional camera picture-in-picture composition, explicit frame-rate/pixel-format conversion, encoding/muxing, bounded queue metrics and discontinuity propagation, and draining accepted buffers/finalizing output on Stop.

On drain it accounts for both marked gaps (`AudioPacket::gap_before`) and the trailing `pending_gap` of each ended audio stream (§4.2). It reports readiness, progress/metrics, recoverable warnings, and terminal result. Terminal success includes a finalized local path and measured stats. It is impossible to report success before the muxer/container closes successfully.

### 7.1 Storage/media hand-off (ADR-0021)

- `RecordingStoragePort::begin` returns `StorageReservation { session: StorageSessionId, target: OutputTarget }`. The core forwards the target unchanged to `MediaPipelinePort::prepare`; the composition root never asks storage for paths.
- `MediaPipelinePort::close` finalizes the container and returns `ClosedOutput` for its target; the session is released whether or not finalization succeeds. `abort` releases a pipeline without a closed output (cancel/rollback).
- `RecordingStoragePort::publish(&session, ClosedOutput)` takes the closed output by value; because `ClosedOutput::new` is public, publish-after-close is enforced by contract plus target check, not by the type alone. A closed output for any target other than the session's reservation, or an inactive session, is rejected with `PortError::InvalidInput` before any filesystem call. The core also checks the target before publishing.
- Storage `abort`, media `abort`, and capture `stop` are idempotent; abort after publish never touches the published recording.

## 8. Backpressure defaults for PoC

Do not freeze queue sizes in the public API. Measure them under target resolutions and formats. PoC must record queue capacity/peak occupancy; video received/encoded/dropped and reason; audio frames received and gaps/overruns; end-to-end latency; process RSS/CPU; stop-to-finalized latency.

Initial policy, now implemented by the core data plane (§4.2): bounded video queue with drop-oldest/stale-frame policy; bounded audio queue that rejects the newest block with an explicit gap marker, behind the callback's bounded ring; control and source-loss signalling separate from payloads. The fail/degraded outcome for audio overrun is surfaced in Phase C. Any policy change requires an ADR or update to this contract.

## 9. Cancellation and resource ownership

- Cancellation is cooperative and idempotent.
- Dropping a session must release OS/portal/PipeWire resources through the safe library API.
- Shutdown order: stop producers (sinks released) -> check for source loss -> drain accepted media -> close pipeline (`ClosedOutput`) -> publish the closed output (storage flushes, links, never replaces).
- A timeout or worker panic is a failure with recovery metadata; it is not a completed recording.
- No adapter may detach an untracked worker thread.

## 10. Conformance checklist

- [ ] Safe public API only; no first-party unsafe or FFI.
- [ ] User-mediated source picker and permission.
- [ ] Explicit selected/negotiated source and format.
- [ ] Owned data and validated dimensions/strides/lengths.
- [ ] Monotonic timestamps and discontinuity reporting.
- [ ] Bounded buffers and counters (core data plane; adapters inherit them by using the sinks).
- [ ] Device unplug and cancellation tests; passes the shared contract suites in `framelet_core::contract`.
- [ ] Deterministic stop and resource release.
- [ ] No network access or media-content logging.
