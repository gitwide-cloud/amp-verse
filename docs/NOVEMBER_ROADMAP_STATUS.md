# November roadmap implementation status

Updated November 12, 2026. This tracks delivered work separately from future work; it is not a claim that the entire roadmap is complete.

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

1. Extend recovery to detailed browser selection and absolute multi-monitor placement. Main mixer/EQ/stem settings and auxiliary cue-deck positions are now checkpointed. Repeated long-session/restart soak tests remain necessary.
2. Improve musical-analysis accuracy using representative recordings and reviewed phrase/section ground truth; retain manual corrections and suggestions separately.
3. Complete Companion control/feedback acceptance and reusable button pages after the generic HTTP interface is verified.
4. Refine Library preparation and Lab project restoration/export workflows; keep MP3 Lab separate.
5. Publish a versioned distribution after these desktop refinements pass acceptance. A source commit alone does not update Android or an existing release asset.
6. Refine the existing six-deck compact/expanded interface against the approved visual target; hardware controller verification remains separate from software-only checks.

## Checks for the recovery implementation

Unit tests cover invalid positions/cyclic layouts, different-library rejection, local database round trip, missing-source retention without Auto DJ restart, and monitor size constraints. The actual desktop crash/relaunch test uses generated audio in an isolated library and settings folder. No audible playback is needed for this test.



## November 12 reliability pass

- Analysis revision 5 records local beat-phase stability over 16-beat windows. Sparse/ambiguous data is unknown; sustained drift marks the constant grid unreliable. Such grids no longer snap generated energy/breakdown cues or produce counted 64-beat markers, drive scheduled blends, force tempo matching, or enable grid-dependent tricks. Manual/reviewed corrections retain priority. This is conservative drift detection, not semantic downbeat/phrase recognition or a variable-tempo tracker. Reanalysis is explicit; installed playback snapshots are unchanged.
- Key Lock synthetic frequency tests now cover both 0.84x and 1.16x tempo endpoints as well as unity. These measure pitch preservation after settling, not subjective quality on a representative music collection.
- Sessions include both auxiliary cue decks' loaded paths, source positions, volumes, modes and output selections. Recovery prepares them paused, refuses to replace actively playing cue decks, and retains incomplete recovery for retry. Legacy checkpoints still load.
- Android discards queued work as soon as a connection is lost or replaced and rejects state responses from a restarted host. Automatic network reconnect uses capped 3–30 second backoff and survives repeated failed handshakes; authentication/incompatible API failures require manual correction. Requests already sent cannot be recalled and are never automatically replayed. Smart Playlist readers now handle Rust's real flattened preparation schema plus legacy phone nesting.
- Headless radio retries up to 16 candidate files when preparation fails, using temporary exclusions without changing saved rules. Individual decoder preparation and encoder-input stalls have timeouts. A finite three-second real FFmpeg test verifies repeated handoffs around a broken source and clean shutdown; this is not an overnight endurance result.
- Shared original audio has MIME types, HEAD responses and bounded single byte ranges, including suffix/open-ended forms and unsatisfiable responses. Unsupported multiple ranges receive the complete file. If-Range falls back to full transfer until strong validators exist. Live radio remains non-seekable.
- Analysis add-on downloads resolve one published stable release version before fetching its executable/checksum, preventing separate latest-version redirects from selecting different versions. Missing/incomplete platform assets preserve the installed engine and provide manual/retry guidance. Downloaded code is not executed by installation.

Verified: 94 core tests, 161 Player tests, 9 Cast tests; native silent auxiliary-deck recovery, generated-audio bad-file failover/repeated encoded handoffs, and a live checksum-verified GitHub add-on download into a temporary directory. Workspace strict Clippy passes. Android 0.2.3-dev (23) is built and installed on Pixel 7; 13 focused Smart Playlist/Remote device tests and lint pass. Optional hardware/live tests remain explicitly ignored in ordinary test runs.

Still open: semantic downbeat/phrase ground-truth evaluation, true variable beatgrids, production music listening comparisons, comprehensive controller/LED/HID support, explicit Android offline-edit publishing/review transport, broader physical receiver matrix, overnight/endurance and Linux hardware acceptance, and a published installer/APK release. A staged Windows build or Git push is not a published release.

## Desktop workflow audit — November 12

Reviewed the import → browse → prepare → playlist → DJ path, plus sampler, Lab, web requests, Cast startup and recovery safeguards. Fixed these concrete gaps:

- Library now exposes Add music folder before indexing. Add folder to queue is named explicitly; the sidebar distinguishes finding music, refreshing cached music and finding analysis inputs. The Library body scrolls in short windows while import and footer status remain reachable.
- Analysis input enumeration publishes newly discovered tracks into the visible index, preserving existing metadata and loaded deck snapshots. An earlier scan cannot replace a newer published analysis/manual revision. Missing analysis-engine setup is offered immediately, before starting a long source scan.
- Preparation tracks the library-facing selection separately from its physical owning source. Windows canonical paths and virtual additional-source paths no longer produce a false busy warning, repeated metadata reloads or missed updates after saving. Derived embedded tags do not cause a metadata reload on every frame. Unanalyzed tracks can use their known file duration in the preparation view.
- Regular playlist creation needs only a name; metadata storage uses a unique internal filename. Create from queue retains queued songs and their order/duplicates. Existing catalog-only playlists are checked for collisions as well as physical files.
- Library stem preparation no longer requires the browsing scan to finish when a source root is already selected. Missing stem setup has a direct entry point. Existing skip-on-failure behavior and completed-stem reuse remain intact.
- A replacement waiting on a playing sampler loop does not block other pad loads. The waiting message identifies the pad to stop, and invalid pad numbers cannot enter the decoder queue.
- An unavailable replacement library root leaves the previous library, queue and session untouched. Cast hosting controls remain available with a cached library during refresh, and its subprocess tool path now uses the platform-native separator.

Verification: 98 core, 178 Player and 9 Cast automated tests; strict workspace Clippy; three web request tests; isolated installed-engine protocol round trip, muted native Auto DJ handoff, real Demucs separation/reconstruction, muted nine-pad sampler playback, Lab MP3 open/waveform/source preservation and repeated encoded radio handoffs. Native screenshots cover Library at 800×460, Analysis and Playlists at 800×600; the preparation screen check exposed and prompted the path-selection fix. New tests cover preparation on additional sources, revision ordering, fresh-index publication, invalid-root retention, named playlist persistence/collision handling, sampler queue blocking and short-window first-run import/status access. In short Library windows, preparation starts collapsed so the song list is visible; its header exposes editing on demand, and taller windows start expanded.

This pass does not claim new Android device acceptance, physical receiver/controller coverage, an overnight radio soak or Linux hardware execution. Existing DSP listening-quality and cross-device/offline-sync roadmap work remains separate. Current user playback was not stopped and no user audio was moved or rewritten.

## Auxiliary deck loading — November 12

The file/drop loading path now accepts A1–A3 and B1–B3, uses the correct deck name, and does not pause regular Player automation when preparing a background deck. An active crossfade only reserves its assigned pair, allowing idle auxiliary decks to load independently. Playing-deck replacement still stages safely.

Verification: 179 Player tests and strict Player Clippy passed. A muted native test loaded all four auxiliary decks while A1 played, preserved Auto DJ and the mix pair, and checked stable layout bounds for 30 frames. Live user-reported flashing was not reproduced or established as fixed; user playback was left running.

