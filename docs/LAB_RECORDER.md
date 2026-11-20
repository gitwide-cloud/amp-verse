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

- Explicit user start/stop; no background recording service.
- Output must not already exist; FFmpeg also refuses overwrite.
- Recordings are written to the destination chosen before capture and are opened in the existing editor when stopped successfully.
- Capture source names and permissions depend on the local operating system.
- Microphone permissions, audio driver/device setup and FFmpeg availability are prerequisites.
- **Not yet tested on physical Windows/Linux devices.** Validate capture start/stop, malformed input, disconnection, file integrity and playback before shipping.
