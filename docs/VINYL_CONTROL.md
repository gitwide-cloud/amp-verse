# Digital vinyl control — accepted requirement, not implemented

The requested vinyl behavior means physical timecode vinyl/CD input controlling digital deck transport, plus ordinary-record passthrough. It is not satisfied by the current mouse jog wheel, scratch effect or MIDI XML importer. This distinction supersedes earlier assumptions that jog gestures alone met the request.

## Current gaps

AMP's mixer has output-device routing and a short bounded reverse-audio history, but no turntable audio capture, timecode decoder, signal calibration, absolute vinyl positioning or normal-vinyl input mixing. The eight-second history is not an arbitrary seekable track cache. Do not expose a working DVS switch or mark a controller as vinyl-compatible based on MIDI import alone.

## Required behavior

- Choose an audio input and stereo channel pair independently for each deck. Never open a default microphone or begin capture automatically. Match phono/line input hardware to the actual turntable setup.
- Calibrate the actual supported record format: input level, left/right orientation, carrier lock, clipping, signal quality and direction. Do not infer record compatibility from generic quadrature pitch tracking.
- Relative mode follows speed, stop, forward/reverse and scratching from a chosen deck position. Absolute mode additionally follows decoded record position and needle drops. Select absolute mode only while the deck is stopped, with a stable validated position, to avoid unexpected jumps.
- Internal mode returns transport to the player. Choose a documented signal-loss policy; distinguish intentional record stop, needle lift, bad signal, disconnection and end-of-record. Test each rather than treating silence as a fixed forward rate.
- Real-vinyl passthrough routes the selected normal-record input through that deck's gain/EQ/fader, headphone bus and live broadcast mix. Timecode input is isolated from all audible buses while DVS is active. Switching to passthrough requires explicit user action and must not silently mix digital playback with the record.
- A DVS-controlled deck has manual transport ownership. Auto DJ must not change its rate, scratch, seek or load another song behind the user's hands. Ownership changes must be explicit.
- Use audio-clock timing, bounded preallocated buffers and independent input/output-clock drift handling. No UI-tick-driven scratching, file decoding, allocations or process launches in the audio callback. Add a random-access PCM cache suitable for reverse motion and needle drops; retain prepared stem alignment.
- Vinyl motion affects a loaded playback snapshot, never portable cues, grids, source audio or saved stem defaults. Cast receives the final audible mix, never the raw timecode tone. No analysis engine is added to Android.

## Implementation and acceptance gates

1. Confirm the user's turntables/controller, audio interface, channel layout and timecode media. No physical DJ controller/turntable interface was available during the initial audit.
2. Evaluate an existing decoder against the chosen media and Windows/Linux targets, with its exact license and version. Mixxx documents xwax as its decoder foundation. No Mixxx/xwax source has been copied or linked into AMP in this work.
3. Verify captured-signal fixtures for forward/reverse motion, variable pitch, stopping, needle lift/drop, clipping, weak signal and reconnects. Measure drift, tracking error and input-to-audio latency instead of claiming a target without evidence.
4. Test relative transport against deterministic seekable PCM, then absolute positioning and prepared stems. Exercise mode changes and Auto DJ ownership while both decks are active.
5. Test passthrough and isolation using deterministic inputs on speaker, headphone and Cast buses. Physical calibration and a scratch/needle-drop listening test on the user's setup are required before declaring the feature usable.

References: [Mixxx DVS manual](https://manual.mixxx.org/2.5/en/chapters/vinyl_control.html), [Mixxx vinyl-control development notes](https://github.com/mixxxdj/mixxx/wiki/vinyl_control), [xwax](https://xwax.org/). Format/device support must be checked against the exact decoder release; a generic “Mixxx compatible” label is insufficient.
