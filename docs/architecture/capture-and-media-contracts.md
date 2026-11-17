# Capture and media contracts

**Status:** Architecture contract for PoC implementation; transport and format choices remain measurable implementation decisions.

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

## 4. Audio block contract

Audio blocks own interleaved samples and include sample rate, channel count/layout, sample format, first-sample monotonic timestamp or sample position on a source clock, number of frames (samples per channel), and discontinuity/overrun marker.

The core normalizes supported sample formats into a pipeline format outside the device callback. The callback may copy into a preallocated bounded ring buffer using a reviewed safe abstraction. It must not allocate a new vector, block on a full queue, or perform conversion/encoding in callback context.

### 4.1 Audio worker transport

`framelet-capture-api` exposes `AudioBlockQueue` as a fixed-capacity FIFO for owned blocks between worker threads. It is not real-time safe and must never be used inside a device callback. Capacity is bounded by block count and maximum interleaved samples per block; sample format is fixed for the queue lifetime. Non-finite samples and format mismatches are rejected. A full queue rejects the newest block, increments a counter, and preserves accepted data. The consumer must propagate rejection as an explicit overrun/failure event.

The CPAL callback's preallocated SPSC ring remains separate. Its consumer worker drains samples, constructs AudioBlocks, and publishes them to the worker queue.

## 5. Time model

- Each source reports monotonic timestamps and a source clock identity.
- At prepare/start, core chooses a recording timeline origin.
- An adapter reports timestamp discontinuities explicitly; it does not reset timestamps silently.
- Media pipeline maps source clocks to the recording timeline and tracks drift.
- Pause creates an inactive interval excluded from output presentation time. Resume starts a new active segment.
- Wall-clock time is metadata for filenames only, never A/V synchronization.

## 6. Events and errors

Adapter events include source ready, frame/audio available, source paused/resumed/stopped, permission denied, selection cancelled, device lost, unsupported format, overrun/discontinuity, and backend failure.

## 7. Pipeline contract

The media pipeline accepts negotiated source configurations and owned blocks. It is responsible for clock mapping, optional camera picture-in-picture composition, explicit frame-rate/pixel-format conversion, encoding/muxing, bounded queue metrics and discontinuity propagation, and draining accepted buffers/finalizing output on Stop.

## 8. Backpressure defaults for PoC

Do not freeze queue sizes in the public API. Measure under target resolutions and formats. Record queue capacity/peak occupancy, video received/encoded/dropped, audio gaps/overruns, latency, process RSS/CPU, and finalization latency. Video can drop stale frames; audio loss must be explicit. Control signaling stays independent from media payload queues.

## 9. Cancellation and resource ownership

- Cancellation is cooperative and idempotent.
- Dropping a session must release OS/portal/PipeWire resources through the safe library API.
- Shutdown order: stop producers -> close producer streams -> drain accepted media -> finalize pipeline -> flush/close storage -> publish final path.
- A timeout or worker panic is a failure with recovery metadata; it is not a completed recording.
- No adapter may detach an untracked worker thread.

## 10. Conformance checklist

- [ ] Safe public API only; no first-party unsafe or FFI.
- [ ] User-mediated source picker and permission.
- [ ] Explicit selected/negotiated source and format.
- [ ] Owned data and validated dimensions/strides/lengths.
- [ ] Monotonic timestamps and discontinuity reporting.
- [ ] Bounded buffers and counters.
- [ ] Device unplug and cancellation tests.
- [ ] Deterministic stop and resource release.
- [ ] No network access or media-content logging.
