# Framelet — arc42 Software Architecture

**Version:** 0.4 — bounded worker-level audio transport  
**Date:** 17 November 2026  
**Status:** Proposed architecture; capture and media technology choices remain subject to PoC gates.

> The repository contains a Slint shell, safe capture data contracts, isolated Wayland and CPAL microphone PoCs, and a backend-neutral bounded audio worker queue. Native capture has not been integrated into the production application; media processing, storage finalization, and real-device validation remain open.

## Audio transport status

`framelet-capture-api` provides a bounded FIFO for owned audio blocks between worker threads. It validates negotiated sample rate/channel count, enforces a maximum block size, rejects non-finite samples, and reports full-queue rejection without overwriting accepted audio. It is not real-time safe and must not be used from a device callback. The CPAL callback remains isolated behind its preallocated SPSC sample ring.

All other architecture sections remain as previously specified in this document.
