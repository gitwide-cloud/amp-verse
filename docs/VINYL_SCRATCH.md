# Manual vinyl scratching

The platter controls the loaded recording itself: touch holds the audible cursor,
forward/backward movement changes direction and pitch with hand motion, and release
continues from the heard cursor using the previous play/pause state. The opposite
deck keeps running. A held platter is silent; it is not a separate scratch sample.

[VirtualDJ vinyl mode reference](https://virtualdj.com/manuals/virtualdj/appendix/optionslist.html).

## Desktop audio handoff

Normal deck voices share pitch-processing look-ahead. The published position is
the audible cursor, which is behind the decoder readhead. On manual touch the mixer
aligns the readhead to a captured audible anchor once, without playing a reverse
movement to get there. Manual vinyl then bypasses the old look-ahead audio.

On release, preallocated dry delay storage is filled with fresh forward source
frames, without running a burst of pitch DSP or adding a second voice. The normal
pitch processor warms through the usual streaming path. If the decoder is late,
the published cursor stays at the release position until the delay is ready. EOF
padding is counted so only the remaining real audio drains. A paused release does
not resume playback until Play is requested.

Gesture phase and rates remain consistent throughout one render callback. A release
uses normal speed even when separate control atomics are between updates. Automated
scratch-in and tape-brake presets retain their existing timing path.

## Regression coverage

`mixer::vinyl_tests` uses real Key Lock processors with Key Lock enabled and disabled.
It checks stationary touch/hold, reverse movement, repeated grabs, release continuity,
other-deck isolation, paused release, EOF, decoder underflow, mid-callback release,
and split control flags. Existing pitch/timing tests cover the normal processing path.
Generated tests establish signal/cursor behavior; physical touch/audio latency still
requires acceptance on the installed desktop and Android device.
