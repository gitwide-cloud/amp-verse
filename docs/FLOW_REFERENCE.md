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

## Implemented section scheduling milestone

AMP now exposes section/energy strips on each of its two decks. A paused click selects without playing; a playing click prepares an independent incoming source voice inside that deck. The other deck is not borrowed. Replacing/canceling a queued selection is supported. Current metadata snapshots and live stem levels remain in use.

Launch mode is saved with transition preferences: Immediately, Next beat, or Next bar. A next-bar grid requires explicit `mix_bar_anchor` preparation on both sources; otherwise the scheduler uses beat phase. Library's “Mark bar 1 at cursor” creates that reviewed anchor. Unknown grids use a ready-first unquantized handoff. Constant-tempo phase matching accounts for the selected incoming section's phase; boundaries are executed per sample in the mixer after prebuffering. The seconds-based fade is unchanged, rather than being stretched to finish on a bar. Thus this is not an exact reproduction of Flow's observed end-of-fade quantization.

Auto DJ uses the existing energy-based candidate/entry selection and now reserves grid launch time before the outgoing end. Effects wait for actual incoming playback, rather than starting during the scheduled wait. Live section selection retains Auto DJ and suspends automatic handoffs during the intra-deck change. Failed preparation preserves current playback; buffer exhaustion during the local section fade rolls back to the outgoing source. Explicit seek/reload/stop and scratch gestures cancel stale pending selections.

Verified with deterministic PCM: exact launch sample, retained fade length, both-deck isolation, rapid replacement, cancellation, stale generation rejection, preparation failure, short-buffer rollback and a bounded continuity check. A muted native test using the supplied MP3s verifies scheduling, source origin, stationary/reverse/released jog motion and unchanged metadata; a separate real energy-based two-deck handoff keeps Auto DJ enabled. Audio quality still needs the user's listening assessment. Variable-tempo tracking, automatic musical downbeats and arbitrary needle-drop access remain outside this milestone; reverse scratching uses the bounded eight-second decoded history.
