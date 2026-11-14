# Two-deck layout — 1.0.19

The two- and six-deck views now share the same responsive performance layout.
Two-deck mode omits the A1–A3/B1–B3 strips and gives that width to the two live
deck panels. It no longer has a second, separately scaled 780-pixel mixer scene.

Jog wheels sit next to the central mixer; the stem/EQ banks sit on the outer
sides. Knobs scale within a bounded 32–44 logical-pixel range as the deck area
changes height. The monitoring/transport controls retain their mirrored order.
Four hot cues and favorite sampler pads sit below each deck; eight cues and all
nine sampler pads remain available through the existing expansion/bank controls.
The real auxiliary Cue Deck remains in **Bank / Cue Deck**. Stem preparation,
gain/filter/routing, Stop and staged-track application remain under **Controls**.

Two-deck waveforms have a shared height divider with remembered sizing. The
library divider can grow the list without an oversized knob bank preventing the
deck area from shrinking. Ordinary resizing does not overwrite six-deck strip
width preferences or change the playback owner.

Verification includes narrow/short/fullscreen geometry, long titles and deep
folders, mirrored headphone/Cue placement, equal A/B wheel sizes, knob placement,
library divider dragging, and existing waveform/deck drag-and-drop tests. Native
screenshots are captured in isolated test profiles rather than replacing the
user's live DJ session. Windows is exercised locally; Linux shares the renderer
but has not been run on a Linux desktop for this change.
