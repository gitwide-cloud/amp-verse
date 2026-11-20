# AMP Lab audio recorder (preview)

AMP Lab's **Waveform editor → Record audio (authorized sources)** provides an opt-in, manual capture workflow:

1. Configure a local audio capture source.
2. Click **Record to WAV...** and choose a *new* output filename.
3. Click **Stop and open in editor**.
4. Trim, fade, normalize, preview and export 256 kbps MP3 using the existing AMP Lab waveform editor.

Audio is recorded as 24-bit PCM / 48 kHz WAV, avoiding generation loss before editing. The recorder does not download, scrape, or extract streams from any service. Capture only audio for which you have recording rights or permission.

## Windows

Requires FFmpeg available to AMP Verse and a DirectShow capture device provided by the system. Use `ffmpeg -list_devices true -f dshow -i dummy` to identify devices. Enter the device's exact name, for example `Stereo Mix (Realtek(R) Audio)` **if** exposed by the audio driver.

**Limitation:** Stereo Mix may be unavailable or disabled, and DirectShow is not equivalent to universal WASAPI system-audio loopback. Native WASAPI loopback is separate future work.

## Linux

Requires FFmpeg built with PulseAudio input and a PulseAudio/PipeWire Pulse server. `pactl list short sources` lists available input and monitor sources. Use the exact monitor source name to capture a permitted system output, or `default` for the default input.

## Behavior and limits

- Explicit user start/stop; no background recording service. Leaving AMP Lab requests Stop.
- Output must not already exist. FFmpeg writes only an exclusively created private staging WAV beside the destination. Publishing uses atomic no-clobber persistence, so a destination created during capture is also protected; the captured staging WAV is retained for recovery in that case.
- Recordings are written to the destination chosen before capture and are opened in the existing editor when stopped successfully.
- Capture source names and permissions depend on the local operating system.
- Microphone permissions, audio driver/device setup and FFmpeg availability are prerequisites.
- Start/stop and process polling run on a capture worker, not the UI thread. Stop is nonblocking. Explicit module/app shutdown waits at most three seconds for acknowledgment; after two seconds the worker terminates a capture that ignores graceful Stop. Shutdown timeout is reported rather than presented as successful recording. Normal module disposal has the same bounded fallback.
- FFmpeg diagnostics are drained continuously with a 16 KiB cap and included in capture errors.
- Five nonignored capture tests exercise the same worker with FFmpeg lavfi (never microphone/device capture): start/stop and lossless stereo 24-bit/48 kHz WAV plus decode, existing/racing output protection, diagnostic errors and nonblocking Recorder disposal, explicit shutdown with a reaped child before return, and rejection of header-only/truncated WAV payloads. Run `cargo test --locked -p amp-player --bin amp-player lab::recorder::tests -- --nocapture`. These tests use no physical input or output device, require FFmpeg, and are intended for Windows and Linux CI.
- **Not yet tested on physical Windows/Linux capture devices.** Device permissions, malformed device names, disconnection and audibility still need hardware validation.

Publication checks a bounded RIFF chunk parser for actual complete stereo 24-bit/48 kHz audio frames, not file/header size alone. Zero-duration captures are rejected; recorded silence with real frames is retained.


Windows fixture readiness reads actual bytes through a fresh file handle rather than cached directory-entry size. A same-device generated-audio probe verified that directory size could remain zero while a fresh handle read 262144 bytes, with the private staging handle still open and FFmpeg exiting successfully. The production staging ownership is unchanged. Readiness keeps its five-second limit and returns the capture worker's actual early-exit diagnostics; it does not silently discard completion failures.
