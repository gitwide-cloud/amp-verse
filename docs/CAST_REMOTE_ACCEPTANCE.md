# AMP Cast and Android Remote — implementation and acceptance

## Player playlist continuation and song fades — November 12

Switching from DJ with the Player Play button adopts the currently active DJ song, including a B-side song, and carries the selected Smart Playlist profile into normal Player continuation. Regular playlists keep queue order; Smart Playlists reuse their source, genre/analysis filters, energy preferences and repeat policy to choose songs. DJ phrase scheduling, scratch transitions and Auto DJ transport stay off while Player owns playback. Selecting a regular playlist explicitly replaces the Player Smart source. Play playlist consumes the first queue entry so it does not accidentally repeat once before advancing.

Normal Player starts use complete songs instead of automatic DJ cue-in/out bounds; approved manual cue boundaries are retained. Player playback copies do not write these changes into shared analysis. Each song fades in at its start and fades out at its end. The next file then starts and fades in; this is sequential song fading, not an overlapping two-deck crossfade or a gapless guarantee. Decode/network preparation can add a pause between files. Fade time is saved and adjustable with the stepper under Player view, sharing the existing duration preference. A separate smoothed DSP envelope leaves channel gain and the volume setting unchanged and resets to unity in DJ mode.

Player's Smart source/recent history and continuation preference are included in paused recovery. Pause/resume preserves playlist advancement. Failed Smart candidates are excluded for the run; decoder failures advance safely when queue playback is enabled. Source metadata and manual cues remain unchanged.

Verification: 98 Core + 191 Player + 9 Cast = **298 passing workspace tests**, strict workspace Clippy, six web checks, and four muted native tests. The new real-decoder check runs a short regular playlist through completion, pauses/resumes it, switches a B1 Smart song into Player, advances to another eligible song and stops according to the repeat policy without enabling Auto DJ. Unit/DSP checks cover filter retention, replacing Smart with regular sources, manual cue protection, fade endpoints and attenuation without gain rewrites. Physical-phone and listening-quality acceptance were not repeated.

## Single desktop playback owner — November 12

Player and DJ use one existing desktop mixer engine. `playback_owner.rs` now owns the Player/DJ handoff policy; there is no remote-owned player or new playback engine. GUI transport, MIDI Play, explicit remote deck starts, Smart Playlist starts and scratch starts call this policy. Player starts pause DJ voices, clear pending DJ automation/scratch restoration and stop DJ sampler/Cue Deck output. DJ starts stop Player queue advancement and pause Player's A1 when launching a different deck; an intentional scheduled DJ blend may adopt that already-playing track as DJ's outgoing voice. Paused preparation on A2/A3/B2/B3 does not switch owners. Starting another DJ deck keeps the current DJ session and mix pair.

Remote `deck:auto` Play/Pause/Stop routes to the current owner. Explicit deck starts target DJ. Player Next/Previous use Player; generic Next follows the owner rather than a stale retained Smart Playlist. The browser shows the current owner and uses DJ Mix Next even when Auto DJ is off. A stopped/finished loaded track can be restarted through the same transport entry point. Stopping the inactive Player view does not stop DJ's A1. Android's existing commands use this backend routing; no APK change is needed for this policy, and no new Android device acceptance is claimed.

Owner choice is included in remote state and local recovery snapshots; old snapshots default to Player. Recovery still restores paused without starting audio. Mixer Pause retains the source/position while cancelling queued launches and sections. Ordered Play/Pause messages prevent an earlier pause from undoing a rapid mode switch. A callback-level test checks cancellation beyond the quantized launch boundary and retained position.

Validation: 98 Core + 187 Player + 9 Cast = **294 passing workspace tests**, strict workspace Clippy, six Node web checks, and three explicit muted native checks (Player/DJ and remote handoff, auxiliary controls during a crossfade, and auxiliary loading/sections/layout). Hardware/phone and long-duration acceptance are separate. The Windows update is staged; current user playback was not stopped.

## Desktop access shortcuts — November 12

Use **Remote / Web UI** above the Library content, at the top of Devices, or beside DJ Library. A resizable window provides direct links to browser remote, song requests and the separate AMP Cast Web UI; it also shows a copyable LAN URL and Android Remote → Discover instructions. Connection settings moved out of the Sources form into Advanced in this window. Start/restart/stop affects only the control service, preserving audio. Existing LAN binding/password policy is unchanged. The suggested LAN link uses the local default-route address (no UDP packet is sent to select the route), falling back to the computer name; multiple adapters/VPNs may require the PC's alternative LAN address.

Browser remote: `http://PC-LAN-IP:8790/remote`. Song requests: `/requests`. Cast station Web UI: the selected Cast server, normally port 8787. Remote sends commands to Player/DJ on the PC; it does not stream audio to the browser. The browser offers active-deck Play/Pause, Player Next or Auto DJ Mix Next, Auto DJ on/off, volume steps and crossfader. It polls live state, uses the same password/session validation as Android, and disables commands on connection loss without stopping playback. Auto DJ still requires a valid configured playlist. Passwords are not placed in URLs or stored by the page. The root request-page URL remains compatible.

Verification: 182 Player tests and strict Player Clippy passed; HTTP route/authentication coverage includes the new page/script. Five Node checks cover remote connection/state, authenticated command dispatch, crossfader values, disconnect/reconnect and existing song-request selection/filtering. This addition has no new physical-phone or receiver acceptance claim.

Date: 2026-11-11. Status: core network/audio workflows and the Remote foreground/background lifecycle are verified on Windows and a real Pixel 7. Platform/output limitations remain listed below.

## Root causes

- Desktop Companion/control defaulted to `127.0.0.1:8790`, preventing LAN phones from reaching it.
- Android Remote had no discovery search and no live WebSocket subscription. Its displayed state changed only after individual HTTP requests.
- Remote's energy slider sent 0–1, while the engine requires 1–10.
- Remote did not verify a protocol/session identity before enabling controls.
- Cast playlist discovery depended on loose JSON files and existing playlist directories. Catalog-backed Smart Playlists and regular playlists could disappear from its source list.
- Headless radio selected intelligent candidates but encoded each cue-trimmed song separately, without an overlapping crossfade or one continuous encoder timeline.
- Android Cast put Start/Stop controls in the connection panel that it hid after connecting.
- Android's playlist “Play on this phone” action also restarted server playback, instead of joining the current timeline.
- The packaged FFplay has networking disabled. Server monitoring therefore needs Rust HTTP transport feeding its decoder through a pipe.

## Implemented changes

### Live Broadcast

The existing bounded master-output taps and MP3 uploader remain the source. No second deck engine is started and no songs are reloaded for broadcasting. The stream contains Player/DJ audio, both main decks, mixer crossfades, EQ/effects already processed by that mixer, prepared stem mixing, samplers and Auto DJ output. Private Cue Deck/headphone output is outside the tap.

The Web UI and Android Cast can request Live Broadcast from Player/DJ on the server computer, supplying its control password separately when configured. The existing desktop sender can still target a selected LAN Cast server. Missing source hosts and rejected passwords produce an error. On-air status waits for actual live audio. Closing the management viewport does not stop the source or server.

Radio worker generations fence old encoder output and old finalizers when a station is replaced. Sender leases still reject stale live sessions. The MP3 encoder disables its bit reservoir so listeners can join an ongoing stream without depending on earlier frames.

### Smart Radio

A saved Smart Playlist or regular playlist can launch one independent station. Sources are loaded from the current library catalog; no loose JSON file is required.

A single continuous MP3 encoder serves every listener. The server prepares the current and next track in device-local temporary PCM files, streams them on one paced sample clock and overlaps them using the same equal-power crossfader gains as desktop DJ. The DJ beat/phrase-grid timing module has moved into AMP Core and is shared by desktop DJ and radio.

Smart Radio reuses AMP Core's prepared-metadata ranking, Hype/Steady/Chill rules, energy targets, progressive energy and recent-repeat/fallback rules. It honors prepared cue boundaries, uses available beat phase for alignment, and matches compatible tempos within an 8% range. Other pairs use a safe timed crossfade. No analysis runs in an audio callback, and playback does not write preparation metadata.

### Management and listening

The existing Web UI retains its theme and now exposes station type, name, saved-playlist selection, Start/Stop, Listen, Play on Server and Google Cast. Playlist editing stays in Library/DJ. Its JavaScript passes syntax checking.

Play on Server joins the station's existing MP3 stream through Rust HTTP transport and pipes it to FFplay; it never reloads the source songs or restarts the station. Stopping the server monitor leaves the station running.

Android Cast keeps its controls visible, authenticates access before reporting a control connection, can launch saved radio playlists or the host's live sender, and can request server monitoring. Listening joins the existing stream. Offline playlist/audio/preparation download remains separate and unchanged.

### Android Remote

Remote Search discovers desktop control hosts and Cast stations. Cast selection routes to the station controls; desktop selection supports the existing explicit password/Connect flow. Manual IP/hostname entry defaults to control port 8790 when no port is supplied.

The authenticated `/api/v1/info` handshake verifies the protocol and current host session. Commands and WebSocket connections identify that session, so a stale client cannot command a replacement session. Existing request IDs and deduplication remain intact. A control-service restart retries discovery binding rather than silently losing its advertiser.

Remote receives live WebSocket state, displays current playlist/master level, both deck states and Manual DJ Lock status, and hides controls on connection loss. Reconnection fetches fresh state without replaying commands. Credentials are retained only in process memory for a foreground/background reconnect, not written to Android preferences.

Player controls include playback, previous/next, relative seek, master volume, regular playlist loading, shuffle and repeat. DJ controls include saved Smart Playlists, Auto DJ, Mix Next, Auto Crossfade settings, style, correctly scaled energy, progressive energy, song requests and saved cues. Existing auxiliary-deck/sampler controls remain available. Switching control views does not intentionally pause local Android playback or issue host Stop commands.

### Deployment scope

ARM32/QNAP build and publication jobs have been removed. No QNAP deployment was performed. Ordinary NAS library support, coordination and portable Windows/Linux server code remain. Sager deployment remains future work.

## Verified behavior and devices

- Windows desktop: full Rust workspace tests pass (191 passed, 34 intentionally ignored); strict Clippy passes; optimized Player and Cast builds succeed.
- Actual HTTP/encoder regression: authenticated start/stop, two listeners, decodable MP3, track transitions, Companion request-ID deduplication, manual lock and WebSocket reconnect pass.
- Catalog regression: Smart and regular playlist sources are found after conversion to a catalog database without loose JSON documents.
- Pixel 7 over actual Wi-Fi/LAN, without ADB reverse: discovery of both control and Cast hosts, manual URL access, incorrect-password rejection, authenticated state, playback/volume/seek, playlist loading, Auto DJ, style, target/progressive energy, song requests, WebSocket updates, stale-session rejection and remote disconnect isolation pass.
- Pixel 7: the Remote screen authenticates, receives live state, reconnects after backgrounding, and leaves ongoing phone-service playback uninterrupted. All five physical-device acceptance tests have passed.
- Pixel 7: Android's actual audio renderer plays independent Smart Radio, and separately plays the existing Live Broadcast master while desktop playback continues. Playback is muted in these automated renderer tests.
- Generated-tone capture: the decoded network stream contains a real two-song Smart Radio overlap. Live captures contain the main deck, a sampler pad and both sides of the crossfader. The private cue frequency is absent from the master capture.
- Server output: the server audio-monitor process stays running on the system sound output, joins the existing station and can stop without stopping the station.
- Independent lifetime: Smart Radio keeps serving audio after the desktop process exits.
- Bigo speaker: real Google Cast discovery, receiver `PLAYING` acknowledgement and confirmed stop pass using the native desktop sender. This is hardware verification, not a compile-only claim.
- Android debug APK/test APK builds and lint pass. Lint reports existing warnings and Google SDK Kotlin-metadata diagnostics but exits successfully with zero lint errors.

The first discovery attempt returned no hosts; subsequent direct/broadcast checks and repeated actual Wi-Fi tests passed. Search now allows five seconds for LAN replies. One initial progressive-energy test failure was a generated fixture's incorrectly nested energy field; it was corrected rather than relaxing engine validation.

## Remaining limitations / acceptance items

- One station per server process. Multiple independent server processes retain separate identities.
- Server monitoring currently uses the operating system's default sound output. There is no per-station named sound-card picker in this update.
- Smart Radio shares selection, timing and gain components with DJ; advanced desktop scratch routines, effects automation and stem mashup orchestration are not all ported into the headless radio runner.
- Temporary PCM preparation is bounded to two hours of source audio per track and needs enough local temporary-disk space for the current/next song. A missing next source is reported after allowing the current cached audio to finish; this is not a general NAS audio-cache service.
- The station's track index is a server-start snapshot. Adding new audio files requires refreshing/restarting that server; choosing a saved playlist reloads its current rules against the indexed tracks.
- Web sender availability depends on the browser/Google Cast SDK and a LAN-reachable stream address. Android and Web receiver-picker flows were not separately hardware-qualified; the native sender was tested on Bigo speaker. Other receiver models and the isolated Garage subnet are not claimed as verified.
- No Sager/Linux hardware deployment was attempted. Linux CI/build configuration is retained; Windows is the locally executed server platform.

## Build, backup and release status

Source was archived before edits, and installed Player/Cast binaries were backed up before replacement. Tests use generated tones and isolated local configuration; the user's NAS music and saved playlists were not edited.

Android preview version is **0.1.18**, version code **19**, installed locally on the Pixel 7. Updated desktop binaries are installed locally and staged for the local updater. The APK is ready for the GitHub preview release after acceptance; public release status is recorded in the delivery message. GitHub-based release/update mechanisms are preserved.

Code and this report are committed to the existing repository. See the accompanying commit reference in the delivery message. Test sources: `LanCoreDeviceTest.java`, `tools/smoke_live_radio.py`, existing Cast/Companion smoke tests and Rust unit tests. Captured generated-audio evidence is kept under ignored `test-output/cast-remote/` and is not published with user data.
