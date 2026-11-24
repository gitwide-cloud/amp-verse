# Manual DJ decks and LIVE assignment

Six-deck mode displays six complete, independent cards: A1/A2/A3 on the left,
B1/B2/B3 on the right, with the crossfader between the banks. Each card keeps
its transport, vinyl, volume and EQ/stem controls. Short windows scroll each
bank independently instead of replacing four decks with tiny selectors.
Two-deck mode remains available in DJ Settings.

Press **LIVE** on a deck to assign that deck to its crossfader side directly.
The highlighted LIVE buttons and center labels show the assigned pair.
This is manual takeover: Auto Mix is turned off, including any queued handoff,
but playing sources are not paused, restarted, reloaded or sought. The fader
position and channel gains remain unchanged. Routing changes ramp over 10 ms.
LIVE does not start a paused deck; use its Play button to start it.

Other playing decks retain their independent channel-volume controls, so three
or more decks can play together. A deck leaving the crossfader pair returns to
its independent full-level bus; set its channel volume deliberately before
reassigning if you want it quiet. Headphone cue remains private.

The separate strict pair-assignment API retains its paused-deck checks for old
session/control callers. The visible LIVE buttons use the live routing command,
so manual performance does not require the old Settings/assign/pause sequence.

Window resizing preserves loaded tracks, source positions, drafts, output and
saved splitter preferences. Card layout and button-hit tests cover both themes;
a clocked silent-output test exercises the actual A2 LIVE button with three
real decoded sources. Synthetic routing tests separately cover six playing
sources and a takeover during an automatic handoff, including same-pair takeover.
