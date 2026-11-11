# Flow 8 Deck workflow inspection

Inspected the user's installed Flow 8 Deck on 2026-11-11 using its Prepare, Play, beatmatching settings, pressing-Play settings and Export screens. This is a record of observed behavior and a proposed AMP direction, not an implementation claim or access to Flow's proprietary analysis code.

## Observed behavior

- Prepare shows a full-track waveform divided into sections, with a numeric energy rating under each section and key above it. Section boundaries and individual beatgrid lines are distinct. The selected example had repeated energy values across adjacent sections, so those boundaries cannot be assumed to be energy-threshold crossings alone.
- Check Downbeat zooms into the beatgrid and begins a preview. The preview was paused after inspection. Split Segment and Mark as Personalized are separate controls; neither was applied.
- Play shows overview waveforms, section/energy strips, key and a shared tempo. Selecting a location on a paused deck moved its position but did not start it. Selecting a section on the playing deck relocated playback to that region. Output was muted for the playback interaction test: this does not verify audible smoothness, phase error or latency.
- Beatmatching settings distinguish the same segment from another segment. The installed configuration finished transitions on four-beat bar boundaries. Same-segment settings showed maximum crossfade 0 beats and seek delay 4 beats. Other-segment settings showed crossfade up to 4 beats and seek delay 1 beat, with permission to seek before the chosen segment. These are the user's observed settings, not asserted product defaults.
- Pressing Play offers either predicting the best launch time or starting immediately. There is also an automatic deck-stop option after fading to zero in multi-deck mode.
- The Export screen inspected offered Top 100 / DJ Sets. This inspection did not establish a supported machine-readable export of beatgrids or section analysis.

All test decks were left paused and the master volume was restored to its original value. No cue/grid edits or segment splits were saved. Playback testing may add an entry to Flow's own played history.

## What AMP already has, and what is missing

The optional analysis engine estimates constant BPM and beat phase (`beat_offset`), with confidence and a fitted onset grid. It proposes sustained energy drops/breakdowns and high-energy mix-in cues. These are cue candidates, not full song-structure segmentation or downbeat recognition. The live Sync control currently matches tempo within +/-16%; it does not phase-align decks.

Therefore copying the appearance of a section strip alone would not reproduce this workflow. AMP needs:

1. Reviewed downbeat/bar anchors in addition to BPM/beat phase, and handling for half/double tempo and unreliable/variable grids.
2. Full ordered section intervals with stable IDs, start/end, energy and provenance. Use neutral section names until a human or a sufficiently reliable model identifies intro/verse/chorus/breakdown/drop. Separate algorithmic suggestions from approved corrections.
3. A section strip on each deck showing current and queued sections and their energy. While paused, a click selects the start without auto-play. While playing, a click queues a musical transition rather than immediately reloading the decoder.
4. An audio-clock transition scheduler that predecodes the requested region, aligns phase and schedules launch/fade at a known boundary. Same-song section changes require two concurrent source voices within a deck; they must not borrow or overwrite the other live deck.
5. An explicit incoming-song/section choice for cross-song mixing. The user can see the target before committing. Auto DJ can choose compatible sections by tempo, harmonic key and local section energy, respecting saved preferences and requested songs.
6. Preserve the existing saved seconds-based fade duration. Any optional beat/bar-based mode must show its effective duration and be selected explicitly; quantization must not silently turn the user's three-second setting into a longer blend.
7. Store approved sections/grids in the existing portable `ampdj/catalog.ampdb` with field revisions and review/conflict protection. Android consumes these prepared sections and schedules playback; it does not acquire an analysis engine.

Before this can be called working: render deterministic section changes on grid-aligned audio, test paused/playing clicks and rapid replacement/cancellation, verify no gap or restart on either deck, test incoming-buffer failure, wrong/low-confidence grids, track reloads, prepared stems, shared metadata changes and retained timing settings. Physical listening tests must supplement numeric beat-phase and discontinuity measurements.

## Public references

[Flow's official guide](https://flow8deck.com/howtoguide) describes preparation, editable structure cues and fades between selected parts of a song. [Mixed In Key's product page](https://mixedinkey.com/flow-dj-software/) describes the preparation/play workflow. These describe product behavior; they do not disclose the BPM, structural analysis or key-detection implementation. AMP should implement its own analysis and scheduling, not claim that UI observation recovers Flow's algorithm.
