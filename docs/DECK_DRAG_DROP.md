# Library-to-deck drag and drop (1.0.14 Preview)

Drag a song from DJ Library onto a main jog panel or any A1–A3/B1–B3 deck strip. Title and metadata cells are draggable. In the Library workspace, drag a song title over the DJ tab, keep holding until DJ opens, then drop on a deck. The destination outlines in its deck color.

An open hand identifies draggable tracks and becomes a closed hand while carrying a track. These are vector cursors: Windows maps the normal Grab and Grabbing icons to the same move cursor, so AMP draws distinct shapes instead. Escape cancels the drag. Releasing elsewhere does not load a deck.

Stopped decks load without starting playback. Playing or automation-reserved decks keep their existing audio and stage the replacement through the existing safe-load path. A drop does not disable Auto DJ. Nested sampler targets retain priority over the encompassing jog panel.

The fix uses one clipped, transformed, layer-aware hit test for both highlighting and accepting internal drops. This avoids container responses being hidden by their own waveform/control widgets. Jog panels now register a target alongside the six strip targets.

Verification: real pointer-event tests cover all six physical decks, both main jog and strip surfaces, normal/reduced scales, single-consumer delivery, cursor feedback, and dragging from the actual Library through the DJ tab. Workspace tests and strict lint passed. Installed Windows verification passed for both B3 strip and the main jog panel; Linux native-window interaction is not tested here.

Windows native verification: a DJ Library track was successfully dropped into B3 without starting playback. Fast native drags may deliver press, movement and release in one repaint; the source map now recovers those gestures from raw events.

## Live section mixing (1.0.15 Preview)

The large live waveforms now use the same section scheduler as the deck thumbnails. Click a section to blend into its beginning at the configured grid timing; a paused deck prepares that point. Overlay mode targets B, or A with Shift. Unanalyzed songs blend to the clicked time. Existing protected cue bounds and crossfade guards remain in effect. Section selection does not rewrite saved cues.
