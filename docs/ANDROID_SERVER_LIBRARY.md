# Android server Library owner flow (integration candidate)

Source branch: feature/android-server-owner-flow, based on de9f996 plus frozen Android sync f3ac28f. This is source for review and same-signer device QA, not an installation or automatic deployment.

Full AMP Cast and the standalone Cast companion use the same CastPanel and CastLibraryView. The FullCastPanel subclass owns local playlist downloads; the companion delegates storage and phone playback to AMP Player over the existing same-signer ContentProvider. Standalone server browsing/control remains independent of Player IPC; phone playback/storage requires the matching Player.

Choose a discovered named AMP server to open its Library. Addresses, optional access-code entry and raw diagnostics remain under Advanced. Open trusted-LAN servers require no code; a configured server password remains enforced. Selecting/browsing a server does not start audio. Exact previously authorized base/server/library credentials may be recovered from AMP Player's encrypted Keystore record; discovery cannot migrate credentials to a different address or identity.

## Playback ownership

- Tap a song to stream it immediately on the selected output. Phone output uses private individual files in AMP Player and follows Android's current audio device. It does not call server Play, start/stop its station, or create an offline file.
- Server-speaker output calls POST /api/player/play with library_id, exact relative paths for direct songs or the exact playlist_id for saved lists, shuffle:false and server_speakers:true. Saved lists are resolved by the server rather than expanded into the direct-song request limit. It requires player_queue:1 and player_track_refs:library_path_v1. Actual host device availability is checked by the server; Windows physical sound still needs real-device verification.
- Phone normal-playlist playback uses a read-only ordered server snapshot, retains available-member order and duplicates, and reports unavailable references without deleting them from the saved document. A missing first resolved reference fails its HEAD check before replacing the phone queue. No title matching or silent source remapping occurs.
- Phone Smart playback uses GET /api/player/playlist with playlist_snapshot:library_path_v1, explicitly labeled eligible_snapshot. It is a fixed eligible sequence, not continuous native Smart DJ ordering. Server Smart playback keeps the server's existing continuous Smart selection.
- Stop targets phone library/radio listening or the selected server program. Other phone playback is retained when the phone radio/library stop action is used.
- Remote opens the selected server's web player in the browser. Native headers and credentials are not put in the URL; browser sign-in remains a separate remembered Studio session.
- Server Play allows 45 seconds for bounded server preflight. Failed/timed-out commands are not automatically resent; inspect status before repeating.

Server/library bindings apply in both open and optional-code modes. An empty credential is legitimate when password_required is false; identity is not treated as a secret.

Private file streaming uses GET/HEAD /api/player/audio?library_id=...&path=..., capability private_audio:library_path_v1. MediaItems contain public resource URLs and an opaque process-local binding identifier. Only AMP Player's media-source factory attaches the selected private headers to that exact resource, using a direct client with redirects disabled. Ordinary Internet radio never receives those headers. Credentials are not serialized with queues. A process restart or expired binding requires selecting the source again.

## Saved playlists and downloads

Browse saved normal and Smart server documents; create either kind after explicitly enabling sync. Normal editing supports rename, add from the current library page, duplicate references, ordering and removal. Smart editing supports name, style and target energy; all other rules, private source hints, companion_available and unknown compatible fields are retained. Full filter editing is available in Remote. New Smart documents use schema 2 even when the server's schema-2 container is empty.

Both apps stage edits through AMP Player's existing durable authority-scoped playlist outbox, captured revisions and CAS contract. Conflicts/uncertain receipts retain drafts. Sync pulls do not mutate active queues, decks or open editors. No deletion/tombstones, metadata or preparation writes were added. Existing Full Library conflict-review UI remains the native review surface; standalone users can open the matching Player to review retained conflicts.

Download is a separate action. Per-song private downloads are audio-only, bounded to 512 MB, cancellable, staged and verified as playable before an atomic publication into Offline playlists. They do not download prepared cues, alter existing downloads, or upload anything. Partial, interrupted, oversized and cancelled transfers remove only their new staged file. A fresh local checksum records the completed audio; this is not a claim of comparison against a remote signed checksum.

Saved-playlist Download uses the existing audio-plus-cues offline workflow. Explicitly requesting it may enable the existing authenticated offline-sharing feature without another settings prompt; classic sharing behavior and server authorization remain unchanged. Existing offline edits/cue conflict preservation continues to apply.

## Verification

The isolated branch's Android workflow runs Full JVM tests, Full/Lite app builds, Full instrumentation-test compilation, Cast/Remote companion builds and lint for both editions. Focused tests cover stable references/order/duplicates, rule and extension preservation, private HTTP identity/header/resource confinement, station-change rejection, and partial/cancelled/truncated download retention, in addition to the frozen sync race/conflict/receipt fixtures.

Instrumentation compilation is not device execution. Real phone streaming/downloads, selected Android output behavior, same-signer IPC and actual Windows/Linux speaker audibility require root's device QA against the matching server API candidate. No deployed installation or full native-addon parity is implied by this document.
