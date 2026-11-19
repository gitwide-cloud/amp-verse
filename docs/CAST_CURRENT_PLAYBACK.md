# Cast current playback (candidate)

This branch targets origin/main aaec29c. Player and DJ use the same master engine on that baseline. The shared NAS checkout described in SAG-47 has different routing APIs and DLNA code; this patch does not copy or overwrite that unsaved work.

Open Cast, inspect the active master title, connect to an AMP Cast server if necessary, find a Google receiver and choose Cast here. Receiver selection starts one master broadcast. The receiver is loaded only after the new broadcast uploads audio, never merely because an independent server station reports running. Opening Cast and server connection do not start playback. Stop casting removes the broadcast tap and requests receiver disconnection without stopping the decks or changing their position, queue or Auto Mix state.

Local-output policy: local playback remains audible during connection, casting, failure and stop. There is no automatic mute; buffered remote output can be heard later than local output. Delay is unmeasured. Do not advertise synchronized speakers or instantaneous handoff. A live stream joins current output; it does not transfer queue ownership to the receiver. AMP Verse must stay running.

## Scope

- Implemented candidate: desktop Player / DJ / Auto Mix master, both main decks, EQ/faders/effects/stems and existing sampler taps. Headphone bus is separate.
- Explicitly unsupported: radio/station listener (ffplay), podcasts, Lab/sample/mix previews, auxiliary side-deck outputs. Active bypass output blocks selection or ends an existing cast; it never silently chooses the main decks instead.
- Browser: existing apps/amp-cast/web/index.html sends its station listen.mp3 URL. It does not capture browser-local playback. Its current handler reports Playing after loadMedia, which is not audible confirmation; no parity claim for this candidate.
- Android: GoogleCastActivity.java loads CastClient.stream() (the station /listen.mp3 URL) using RemoteMediaClient; it does not capture DjEngine or PlaybackService local audio. This separate sender needs acceptance against the agreed integration baseline. DLNA is absent from this origin/main baseline. Neither is claimed tested or implemented here.

## Validation and remaining work

Added source-resolution, startup/failure/cancel and synthetic master-tap continuity regressions. Existing headphone-cue regression checks that the broadcast receives silence when only cue is audible. No physical receiver is used by those tests.

Run under Pochiko's one-build lease with CARGO_BUILD_JOBS=2: cargo test -p amp-player --locked cast_; cargo test -p amp-player --locked google_cast::tests; cargo test -p amp-player --locked headphone_cue_is_pre_fader. Full typecheck and regressions remain required; parse/format validation alone does not prove compilation.

Known review focus: rust_cast's receive/connect API is blocking and does not expose socket timeout configuration here. Stop marks its session cancelled and stops uploading immediately, but receiver stop_app can wait for receive to return. Do not mark reconnect/stop acceptance complete until bounded transport cancellation is implemented and verified. Existing two-device sampler/master tap uses separate device clocks; no drift measurement exists. Startup discards the bounded connection-time tap backlog.

Before ready: Seno code review, Emmy UX acceptance, bounded test results, real screenshots, reproducible GitHub Actions Windows artifact, browser/Android baseline reconciliation and an explicitly authorized physical-device test window. No build on the Windows PC, production restart, release or merge is authorized by this patch. Rollback is to run the preceding artifact; this change does not migrate stored library or playlist data.
