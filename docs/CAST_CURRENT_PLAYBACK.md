# Cast current playback (candidate)

This branch targets origin/main aaec29c. Player and DJ use the same master engine on that baseline. The shared NAS checkout described in SAG-47 has different routing APIs and DLNA code; this patch does not copy or overwrite that unsaved work.

Open Cast, inspect the active master title, connect to an AMP Cast server if necessary, find a Google receiver and choose Cast here. Receiver selection starts one master broadcast. The receiver is loaded only after the new broadcast uploads audio, never merely because an independent server station reports running. Opening Cast and server connection do not start playback. Stop casting removes the broadcast tap and requests receiver disconnection without stopping the decks or changing their position, queue or Auto Mix state.

Local-output policy: local playback remains audible during connection, casting, failure and stop. AMP's volume slider controls the remote master level too; use the PC speaker volume to reduce only local sound. There is no automatic mute; buffered remote output can be heard later than local output. Delay is unmeasured. Do not advertise synchronized speakers or instantaneous handoff. A live stream joins current output; it does not transfer queue ownership to the receiver. AMP Verse must stay running.

## Scope

- Implemented candidate: desktop Player / DJ / Auto Mix master, both main decks, EQ/faders/effects/stems and existing sampler taps. Headphone bus is separate.
- Explicitly unsupported: radio/station listener (ffplay), podcasts, Lab/sample/mix previews, auxiliary side-deck outputs. Active bypass output blocks selection or ends an existing cast; it never silently chooses the main decks instead.
- Browser: existing apps/amp-cast/web/index.html sends its station listen.mp3 URL. It does not capture browser-local playback. Its current handler reports Playing after loadMedia, which is not audible confirmation; no parity claim for this candidate.
- Android: GoogleCastActivity.java loads CastClient.stream() (the station /listen.mp3 URL) using RemoteMediaClient; it does not capture DjEngine or PlaybackService local audio. This separate sender needs acceptance against the agreed integration baseline. DLNA is absent from this origin/main baseline. Neither is claimed tested or implemented here.

## Validation and remaining work

Added source-resolution, startup/failure/cancel, terminal-status, supervised unresponsive-worker and synthetic master-tap continuity regressions. The worker fixture sleeps without contacting any receiver; the supervisor must terminate and reap it before returning. Existing headphone-cue regression checks that the broadcast receives silence when only cue is audible. No physical receiver is used by those tests.

Run under Pochiko's one-build lease with CARGO_BUILD_JOBS=2: cargo test -p amp-player --locked cast_; cargo test -p amp-player --locked google_cast::; cargo test -p amp-player --locked headphone_cue_is_pre_fader. Full typecheck and regressions remain required; parse/format validation alone does not prove compilation.

Receiver I/O now runs in a supervised child process of the same packaged player, before GUI/audio/library initialization. Stop drops the broadcast and requests graceful receiver shutdown; after a two-second grace the supervisor terminates and reaps an unresponsive worker before enabling another selection. Initial connection is limited to 15 seconds; a connected worker with no received Cast packets for 30 seconds is closed. Reports use an eight-entry bounded channel and 4 KiB read limit. A forced disconnect does not prove stop_app reached the device: the UI explicitly says the receiver did not confirm stop and may finish buffered audio. Physical unplug/reconnect and Windows worker packaging still require verification; these are deadlines, not measured speaker latency. Existing two-device sampler/master tap uses separate device clocks; no drift measurement exists. Startup discards the bounded connection-time tap backlog.

Before ready: Seno code review, Emmy UX acceptance, bounded test results, real screenshots, reproducible GitHub Actions Windows artifact, browser/Android baseline reconciliation and an explicitly authorized physical-device test window. No build on the Windows PC, production restart, release or merge is authorized by this patch. Rollback is to run the preceding artifact; this change does not migrate stored library or playlist data.

## Review revisions

Player, compact Player, DJ and Library Devices now provide a direct Cast entry and active indicator. Server connection/start/status is visible without expanding station controls. Receiver lifecycle is the primary status; stream details are separate and do not imply audible playback. Setup hints are hidden during active casting, preparation copy is cleared on receiver connection, and stopped/error copy never wraps itself. Finished sample/timeline/waveform children are reaped when checking output ownership; Track Preparer stem previews are included as unsupported bypass sources. Fully faded/muted main channels are excluded from the displayed active-master title; the master tap still defines the actual audio.

Validation attempt on 2026-11-19: bounded `cargo check -p amp-player --tests --locked --offline`, two jobs and cached target, was terminated with SIGTERM (exit 143) after a concurrent agent workspace build was discovered. Only dependencies had been checked; the player typecheck and test execution were not reached. My process tree was removed and the coordinator notified. Formatting and diff checks passed; this is still an uncompiled review candidate.

Second review corrections: waveform preview completion now shares Repeat/playhead handling; server status is separated from station messages and healthy polls clear transient connection errors; Cast fits existing Player/DJ toolbar rows; worker regression is included by the `google_cast::` filter.
