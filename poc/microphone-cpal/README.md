# Framelet microphone PoC

This isolated evaluation PoC enumerates microphone devices and captures default F32 input through a bounded SPSC ring. It is not a production backend.

## Run

\`\`\`bash
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --list
cargo run --manifest-path poc/microphone-cpal/Cargo.toml -- --record --device 0 --duration-seconds 10
\`\`\`

Recording requires explicit \`--record --device INDEX\`. The index is local to the current enumeration. Only the default F32 input format is accepted.

## Local WAV output

The PoC writes 32-bit IEEE-float PCM WAV files. Pass \`--output PATH\` to choose the destination; otherwise it generates a timestamped \`framelet-recording-*.wav\` in the current directory.

- Files are created exclusively by the WAV writer; existing recordings are never overwritten. Automatic names are candidates only, and exclusive creation is the final collision check.
- The fixed-size ring is drained continuously into AudioBlocks, then written on the consumer thread.
- The WAV header is patched only after capture stops, remaining samples are drained, and no overflow or callback error occurred.
- A completed path is printed only after flush and sync succeed.
- RIFF's 32-bit size limit is checked; crash recovery and temporary-file publication are not yet implemented.
- Output is float PCM WAV, not MP3/AAC/MP4.

## Safety and limitations

- The CPAL callback performs bounded copies only; no file I/O, allocation, logging, locking, or channel sends.
- Ring overflow is counted and causes the run to fail without finalizing the file.
- CPAL remains isolated from the production workspace.
- First-party Rust forbids unsafe code; third-party CPAL/platform internals may contain unsafe/native FFI.
- No format conversion/resampling, pause/resume, A/V sync, UI integration, webcam or screen recording is included.

Real Windows WASAPI and Linux PipeWire/ALSA tests remain required. See \`docs/REAL_DEVICE_TESTING.md\`.
