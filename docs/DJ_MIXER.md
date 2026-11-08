# AMP DJ live mixer

Load a track to Deck A and another to Deck B from the embedded library. Loading prebuffers audio without playing it. Start the outgoing deck with Play. Press **AUTO CROSSFADER** above the center slider to start the incoming deck and transition across. Press it again to cancel, or drag the manual slider to take over. At fade completion the outgoing deck pauses; its position is retained for a return transition.

The Mix value sets 0.5-30 seconds. Transitions use equal-power gains on a single device sample clock. They shorten when either track has less audio remaining, using cue ranges or probed duration. A track still loading its duration, an ended incoming track or a stopped outgoing deck produces a status message. The incoming buffer must be ready before a fade starts. This removes the old independent-FFplay process timing from desktop playback, but cannot guarantee uninterrupted output during device failures or NAS starvation. Buffer underruns are counted and displayed.

## Working controls

- Play/Pause retains decoded position. Cue returns to the prepared IN point and pauses. Stop releases that deck's decoder. Manual and generated metadata cue buttons seek through a replacement decoder; they are not instantaneous sampler hotcues.
- Pitch changes playback speed and pitch together over +/-16%, using interpolation. SYNC adjusts speed to the other deck's prepared BPM, within that range. Beat-phase alignment and key-preserving time stretch are not implemented.
- Each channel has a live gain fader and measured peak meter. The top displays are measured PCM envelopes, not beat-grid waveforms or fabricated animation. Master volume is live.
- Loop captures the next four beats using prepared BPM, then repeats the captured audio. When BPM is unknown, the button explicitly uses a two-second loop. Exit resumes after the captured section. Start the loop with enough remaining audio to capture its full length. Maximum capture is 30 seconds.
- EQ opens three split-band gain controls per channel. FX opens a per-channel low-pass filter; Bypass restores unfiltered audio. Additional FX, headphone cue routing, MIDI and OSC remain future work.

## Audio architecture and verification

A CPAL output callback mixes both voices into one stream. Separate FFmpeg workers decode stereo float PCM at the device sample rate into four-second lock-free SPSC ring buffers. The callback never waits for a NAS read or decoder. GUI controls are atomic values or bounded commands; meters and scope data come back through atomics. A small channel-gain ramp suppresses gain-on clicks; the mixer reserves headroom and clamps output peaks. This is an initial DSP mixer, not a mastering limiter or professional time-stretch engine.

Deterministic PCM tests cover continuous automatic fades, waiting for incoming audio, outgoing pause on completion, pause position, pitch, loop position and manual takeover. An opt-in Windows native-device test uses two real FFmpeg decoder workers and a muted CPAL device stream to check A-to-B and B-to-A completion, pause stability and zero buffer underruns. The native test does not replace subjective listening or long-duration NAS/device-loss testing.

The desktop Player uses this same mixer; independent Cast encoding is unchanged. Android is unchanged.
