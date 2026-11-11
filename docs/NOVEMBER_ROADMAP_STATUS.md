# November roadmap implementation status

Updated November 11, 2026. This tracks delivered work separately from future work; it is not a claim that the entire roadmap is complete.

## Verified baseline

Android Remote, live desktop-master broadcasting, independent Smart Radio and Google Cast were exercised for the 0.1.18 delivery. See [Cast and Remote acceptance](CAST_REMOTE_ACCEPTANCE.md) for evidence and limitations. QNAP application development remains canceled. The normal NAS library remains supported. MP3 Lab remains independent.

## Current desktop refinement

- Compact Transitions popup with Timing, Effects and Advanced Mixing tabs. Existing timing, effects and mashup processing are retained; opening settings does not consume deck height.
- The mixer now scales against the complete channel/fader/stem control height, with more room for channel controls.\n- Show/Hide Library releases the browser's space to the decks without discarding its splitter height.
- Device-local session database: `ampdj/session.ampdb` beneath the application settings folder. Each library root has a separate record. No service is required; recovery does not write shared catalog, playlist or cue metadata.
- A checkpoint is written in the background every five seconds after indexing is available, and synchronously on normal exit. A sudden crash can lose the most recent five seconds plus a checkpoint in flight.
- Recovery offers Restore session or Start fresh. Automatically restore paused next time is optional. Neither mode resumes audible playback, Auto DJ, live broadcasting, samplers or cue-deck playback.
- Deck paths and source positions, queue paths and original playlist revision, pending requests, selected Smart Playlist/rules, style/energy, transition choices, crossfader position, master level, open panels, magnetic parent relationships, window sizes and DJ splitter positions are retained. Existing independently saved sampler assignments continue to load paused.
- Missing decks/queue entries are reported and the original checkpoint remains available for retry. It is not replaced by a partial recovery. A root switch checkpoints the old session before opening the new root's recovery record.
- Window dimensions are constrained to the current monitor. Absolute screen positions and maximized/fullscreen state are not restored yet.

## Remaining priorities

1. Extend session restoration to auxiliary cue-deck positions, transient mixer/EQ/stem controls, detailed browser selection and absolute multi-monitor placement. Repeated long-session/restart soak tests remain necessary.
2. Improve musical-analysis accuracy using representative recordings and reviewed phrase/section ground truth; retain manual corrections and suggestions separately.
3. Complete Companion control/feedback acceptance and reusable button pages after the generic HTTP interface is verified.
4. Refine Library preparation and Lab project restoration/export workflows; keep MP3 Lab separate.
5. Publish a versioned distribution after these desktop refinements pass acceptance. A source commit alone does not update Android or an existing release asset.
6. Six-deck work remains a separate design discussion only; no additional main decks are being implemented.

## Checks for the recovery implementation

Unit tests cover invalid positions/cyclic layouts, different-library rejection, local database round trip, missing-source retention without Auto DJ restart, and monitor size constraints. The actual desktop crash/relaunch test uses generated audio in an isolated library and settings folder. No audible playback is needed for this test.

