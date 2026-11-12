# AMP Verse Android preview


## Desktop library and Android Remote ownership (current direction)

Desktop AMP Verse owns the music libraries, folder roots and playlists. Android Remote connects to that desktop/server, browses its songs/playlists and streams selected content to the phone. From that remote source, the user can explicitly Save offline; that downloaded collection belongs to the radio portion. A phone-folder import is a separate local-player source, not the definition of the Cast offline library.

Target flow: desktop adds library → Android Remote connects → browse songs/playlists → listen on phone → Save offline for Cast. Downloading a song or playlist must copy the actual audio and portable preparation to a managed radio cache, persist membership/source identity and report completion only once playback is available without the desktop. Offline radio items must remain usable when the source is unavailable; refresh must preserve local cue overrides. Do not cache a live station stream and label it as downloaded playlist songs. No desktop playback command should be sent when the user chooses to listen on the phone.

Current limitations: the Android build supports independent phone-file imports and direct radio-stream URLs. Desktop library browsing/streaming APIs, Remote browsing and Save offline from that source are not implemented. Retain existing imported phone audio; do not silently move, delete or relabel it as Cast downloads. Direct phone-folder playback remains available as a separate local-player source, as confirmed by the user.


Package: `cloud.gitwide.ampverse`. Android 9+; compile/target SDK 35; JDK 17; Gradle 8.9; Android Gradle Plugin 8.7.3. Media3 1.8.1 is pinned for this SDK/toolchain preview. Source and app data are separate from MP3 Lab.

## Build

Set `JAVA_HOME` to JDK 17 and `ANDROID_HOME` to an Android SDK containing platform/build-tools 35.0.0 and platform-tools. From `android/` run `./gradlew :app:assembleDebug :app:assembleDebugAndroidTest :app:lintDebug` (Windows: `gradlew.bat`). The wrapper uses the official Gradle distribution. The Android CI workflow builds/lints and uploads the preview APK; device tests require a connected device and are not claimed by that build-only job.

APK: `android/app/build/outputs/apk/debug/app-debug.apk`. It is a debug preview signed with the build machine's debug key; release signing and GitHub Releases distribution are future work. Do not assume APKs from different CI machines can update a locally signed debug install.

## Use

Open **AMP Verse**, then **Library → Import local songs**. Choose one or more audio documents. Imports are copied into the app's local storage; originals are untouched. The queue persists across activity/process restarts. Removing an entry removes it from the queue; copied files are currently retained in private storage. A storage-management UI is still needed. Imported audio is retained until app data is cleared or the app is uninstalled.

The topmost **AMP VERSE** dropdown lists **AMP Player, AMP Library, AMP DJ, AMP Remote, AMP Lab, AMP Cast**, plus **Settings**. A horizontally scrollable module strip repeats the six module shortcuts. Settings contains **Classic**, **Modern Light** and **Modern Dark**, Repeat queue, Android notification settings, local storage information and App Updater. Appearance uses one palette shared by all app screens. Player exposes real seek, pause, previous/next and stop. Queue order plays sequentially; Repeat loops it. The media service supplies standard Android media controls. Do not claim endurance, Bluetooth/device-route coverage or screen-off reliability until verified on hardware.

## Device test / installation

Use `adb devices -l` and target the Pixel 7 serial explicitly. Install the main APK with `adb -s <serial> install -r <apk>`, then the Android test APK. Run `adb -s <serial> shell am instrument -w cloud.gitwide.ampverse.test/androidx.test.runner.AndroidJUnitRunner`.

`PlaybackDeviceTest` preserves/restores the library, generates two silent local WAV fixtures, checks atomic queue persistence, actual decoder progress and track transition, then sleeps/wakes the display and checks playback progress. It restores the library and deletes only its own fixtures. The test may briefly turn the phone screen off. No user audio is required. Initial SAF picker/theme UI checks remain manual until a dedicated UI test is added.

**User instruction: leave AMP Verse installed on the Pixel after development/testing. Never uninstall the main app as cleanup.** Do not clear unrelated app data or modify other installed apps.

Still unbuilt: folder-tree import/browser, named/reordered playlists, prepared cues/beatgrids/stems, manual cue overrides, cue-aware Auto DJ, LAN discovery/password remote control and automatic updates.


## App updater

Version 0.1.1 adds a manual **Settings → App updater → Check for updates** request to GitHub's HTTPS latest-release endpoint. It compares stable three-part version tags, reports unavailable/private releases honestly, and offers **Open GitHub Releases / Download**. This opens the canonical repository Releases page in the browser, where sign-in can happen if required. No GitHub credentials are stored in the app. APK download and installation are user-driven; no silent update or plugin loader is included. The Android INTERNET permission is used only for this explicit update check in the current app. Local playback still works offline.


## Compact Classic shell (0.1.2)

The reference-inspired shell uses a compact metallic/beveled title dropdown and buttons, dark information panels, green elapsed-time/track displays, local icon fallback, seek/transport row and a local queue. Modern Light/Dark reuse the same views and actions with flat surfaces. No animated spectrum, fake artwork, artist metadata or fabricated queue songs are added. The imported folder/count reflects actual local saved tracks.

AMP Player and AMP Library retain playback/import/queue-removal operations. AMP DJ exposes the currently implemented local sequential queue/repeat path, with advanced mixing marked unfinished. AMP Remote, AMP Lab and AMP Cast are navigable preview pages explaining what is not yet implemented on Android. They do not pretend to connect, edit or broadcast. Settings remains the sole theme-selection location and contains the updater.


## Scan music folder (0.1.3)

Use **AMP Library → Scan music folder** (or **Player → + Music → Scan folder**). Choose a folder and approve **Use this folder** in Android's picker. All audio documents in that folder and its subfolders are scanned on a background worker and copied to private local storage for offline playback. No storage-wide permission is requested. **Select individual songs** remains available. Android may restrict selecting storage roots/Downloads; choose an allowed music folder.

Repeated imports of the same document URI are skipped while that source is already in the saved library. This is source-identity matching, not content hashing: the same file selected through a different provider/URI, or songs imported before source tracking was added, may be copied again. The current library is still an Imported collection, not a browsable representation of the source folder hierarchy. The UI initially renders up to 100 songs with a Show more action; all imported songs remain in the playback queue. Formats depend on the Android playback engine; an audio extension is not proof of codec support.


## Player tools (0.1.4)

The player now includes **EQ**, **Shuffle**, **Repeat**, **PL** and **Radio** buttons plus a live 24-band spectrum above the track card. Shuffle changes Media3 queue traversal, Repeat loops the queue, and their selections persist. **EQ** opens five adjustable bands (80 Hz, 250 Hz, 1 kHz, 4 kHz, 12 kHz), enable/bypass and Flat reset; changes are saved and applied in the service's PCM processing chain. Band gains range from -12 to +12 dB; excessive boosts can clip. The current processing path uses 16-bit PCM with float audio-sink output disabled. No microphone/recording permission is requested. Spectrum levels come from decoded post-EQ audio, decay when audio stops and are not random animation; spectrum measurement is skipped while the view is hidden.

**Radio** plays a user-entered direct HTTP/HTTPS audio-stream URL, including an AMP Cast `/listen.mp3` URL. This is internet/network radio, not an FM tuner or station directory. HTTP streams require cleartext networking support, which is enabled for these user-selected URLs. The live stream replaces the active timeline temporarily; the saved local library is retained and choosing a local library song restores the local queue. Radio download caching, a station catalog and native Cast administration remain future work.


## Integrated AMP Lab (0.1.6)

Choose **AMP VERSE dropdown → AMP Lab**. The working MP3 Lab Android editor is integrated as a native activity in this APK, not a link to another installed app and not a rewritten editor. Use **Open** to select audio, then its existing waveform/trim IN/OUT, zoom/pan/nudge, fades, gain, normalization, original/edited preview and MP3 export controls. Editing sessions use the app-private `amp-lab-editing` directory, separate from phone playback imports. Original files are unchanged until you explicitly select an export destination. Lab's top dropdown returns to the other AMP modules or Settings. The three saved themes apply to Lab too.

The source foundation is the adjacent MP3 Lab editor, matching the supplied local APK family; provenance and original source hashes are in `third-party/mp3-lab/`. Only package/navigation/theme/session-directory adapters were added. Existing MP3 Lab source/repository/APK are unchanged. Full multichannel/stems and advanced beat analysis are not supplied by this editor. Android 0.1.6 APK targets arm64-v8a, matching the existing MP3 Lab APK and both development phones, to avoid shipping unused native architectures. Desktop AMP Lab integration remains the external MP3 Lab connection.

## Integrated AMP Cast client (0.1.10)

Open the top AMP VERSE dropdown and choose **AMP Cast**. Enter the running server's full HTTP/HTTPS address (for example `http://192.168.1.50:8787`), optionally enter its admin password, and press Connect. Start/Stop controls only the selected server/station using its persistent IDs. Refresh reads its actual now-playing state, listener count and queue. The queue view shows up to 200 entries. The address is saved; the password stays in memory for that screen session. No discovery, automatic connection, cloud account or desktop plugin loader is added.

**Listen on this phone** plays `/listen.mp3` through the existing foreground Media3 playback service and notification/lock-screen controls. It replaces the active phone timeline, retaining the separately saved local library. **Stop phone playback** stops phone radio listening without stopping the station. **Stop station** is a separate, password-protected server command. The phone does not host a broadcast server. Start `amp-cast` separately on the desktop/Linux server, bind its LAN interface, and use that machine's reachable address; `127.0.0.1` on an ordinary phone refers to the phone itself.

This initial native client covers a single selected station's start/stop, read-only queue/status and phone listening. Desktop library browsing/downloads, offline Cast collections, Google Cast receivers, playback synchronization, station playlist editing/selection and multiple stations remain unimplemented. This section supersedes earlier statements that native Cast administration is wholly planned; the Remote/offline limitations still apply.

### 0.1.11 search and connection flow

AMP Cast offers **Find servers**, then an explicit server selection. Use **Server settings / enter address** for a manual URL and optional password. Discovery is IPv4 LAN UDP, not Internet scanning; servers behind network isolation may require a manual address. Search station songs filters the connected queue. Search phone music filters local playback songs while preserving their queue positions. Phone playback remains separate from Cast. Connection controls only enable station start/stop/listening when their status allows it. All three application themes use the same layout.


## Android 0.1.12 — DJ, playlists and desktop Remote

This section supersedes the older placeholder-DJ/Remote descriptions above. Android remains an integrated app, without a plugin loader. Open modules from the AMP VERSE dropdown; the existing three themes, local player, Cast client and integrated MP3 Lab editor remain available.

**AMP DJ:** service-owned native Media3 A/B decks, amber/cyan loading destinations, pitch ±16%, jog seeking, saved local hot cues, track loop, master and equal-power crossfader, and an automatic crossfade button. Portrait switches the visible A/B control panel while both decks continue; landscape shows both. Controls and the searchable bottom library scroll within the phone. Load playlist selects phone playlists/Smart Playlists for the DJ source. Start Auto DJ prepares an inactive deck and transitions near the end of the outgoing song. Stop Auto DJ retains current playback. The nominal crossfade is eight seconds, shortened for short tracks. It waits for incoming audio before moving the fader.

Each independent auxiliary panel can collapse or switch between Sampler and Cue Deck without stopping playback. Each Sampler has nine square touch pads, independent volume, local saved assignments, rename/replace/loop/stop/clear on long press. Effects are copied into private storage, limited to 8 MB and five seconds for Android SoundPool. Cue Deck has its own Media3 player, transport, seek, local cues, volume and Send to A/B. A playing main/cue deck stages replacements; Apply staged requires it to be stopped. Long-press a DJ Library song to drag it onto a main or cue deck, or use A/B buttons.

**Library:** normal playlists support create, name, select songs, reorder, remove, save, play and delete. Hold a playlist for its menu. Smart Playlists use a whole scanned folder, a regular playlist, or all phone music, plus Hype/Steady/Chill and Auto/Manual energy. An empty source includes no songs. Preview suggested order is a list preview, not an audio audition. Folder labels are recorded from newly scanned SAF trees; rescan a previously imported folder to label existing entries without copying duplicates. Holding a Library song opens local BPM, Camelot/key, energy and rating overrides. Cue slots and these corrections are private phone data and never overwrite NAS metadata on reconnect.

Phone Smart Playlist ordering is a lightweight heuristic over saved BPM, energy and Camelot values; it does not analyze audio or claim parity with the shared Rust analysis engine. Unknown analysis is not filled with fabricated BPM/key. Use desktop-prepared Smart Playlists through Remote for the full engine.

**AMP Remote:** explicitly connect to the desktop control gateway, normally `http://DESKTOP-IP:8790`, with an optional password. The desktop must bind its trusted LAN interface; this update does not change firewall/listen settings. Search the real desktop catalog and load songs or saved cues into A/B or either desktop Cue Deck. Remote controls include transport, preload/mix next, Smart Playlist loading, Auto DJ, crossfade, manual lock, style, target energy and both desktop sampler banks. Existing Companion request IDs/API remain unchanged. Commands carry fresh IDs, are not automatically replayed on reconnect, and rejected operations are reported. Remote controls desktop audio; use AMP Cast separately to listen on the phone.

### Remaining parity limits

Reverse-audio scratching, scrolling DJ waveforms/beatgrids, full analysis/cue detection, separate hardware headphone routing, desktop-only window docking/add-ons, multitrack editing, shared-library metadata editing/sync review, and individual desktop-song downloads/offline Cast collections are not implemented by this update. Jogging seeks; it does not pretend to scratch. Android cue/sampler audio uses the current system output and does not promise a separate headphone bus. The full desktop engine remains on the computer, accessible through its supported Remote commands. AMP Lab retains its existing single-track trim/fades/gain/normalize/MP3 export foundation.

Build: `:app:assembleDebug :app:assembleDebugAndroidTest :app:lintDebug`. The installed preview uses the same local debug signing key as 0.1.11, allowing an in-place update without clearing imports/settings. CI preview APKs use the CI debug key and are not guaranteed compatible with that installation.


## Android 0.2.1 preview — Remote discovery

AMP Remote opens with a prominent Discover devices button and automatically searches the current LAN. Player/DJ and Cast discovery run concurrently, independently of the command queue, with nearby hosts listed directly on screen. Tap a Player/DJ host to connect; password-protected hosts first select the address and request its control password. Tap a Cast host to open AMP Cast with that server selected. Manual address entry remains available. Refresh disables only during the bounded scan, and leaving Remote discards late discovery callbacks. Discovery never starts playback or sends control commands.

Device verification: debug APK and instrumentation APK built and installed in place on Pixel 7. Three discovery tests passed, covering malformed replies, deduplication, sender-derived host addresses, password flags and the actual Remote screen’s automatic discovery/password flow. Tests use an isolated UDP responder and do not play audio.


## Android 0.2.2 preview — Smart Playlist options

Open Library, tap a Smart Playlist, then choose **Genres, analysis filters & advanced options**. The editor provides Hype/Steady/Chill buttons, Auto/Manual target energy, multiple folder/regular-playlist sources and individual songs (union with deduplication and subfolders), genres, all 32 desktop analysis/track filter fields, excluded/preferred songs, recognition-first selection, saved energy progression, recent-song protection, end-of-source fallback, and approved manual build/drop cue preference for Hype. Empty sources mean no songs; an empty genre selection means all genres. Every filter must match, and missing numeric/text metadata does not match a range/text condition. Stem count describes saved preparation entries, not downloaded/usable stem audio.

Filters use the desktop field names, types and scales (energy 1–10, confidence percent); phone overrides take precedence where present. Saved BPM, Camelot and energy are read from synced preparation without running an analyzer. Preview runs off the UI thread; it and Play use the current draft. Advanced Apply updates the draft, Save Playlist persists it, and Cancel leaves saved rules intact. Phone rules remain local and never publish to a NAS on reconnect. This does not add shared-library rule synchronization or the Rust desktop mixing engine to Android.

Loading a Smart Playlist from the DJ source picker applies its options to that Auto DJ session. At queue exhaustion, Stop ends playback; Continue refills from the eligible source, avoiding recent songs when possible, relaxing recency if necessary, and never selecting the currently sounding track. Ordinary playlists remain finite. Recognition is first only if it is in the selected source and passes every filter/exclusion. Preferences affect ordering without overriding eligibility. Hype's manual cue preference uses saved build/drop labels within the saved cue-out.

Verification: Android build/lint and focused Pixel 7 instrumentation tests cover saved metadata scales, missing values, all filter field definitions, manual cue preference, multiple sources/subfolders/deduplication, exclusions, recognition, local overrides and persistence. Test fixtures use a private temporary catalog and namespaced preferences. No phone analysis engine is bundled.


## Android 0.2.3 preview — connection and preparation compatibility

Queued Remote commands check their original connection generation before network dispatch; leaving, disconnecting or selecting another host discards commands that have not started. A command already sent is not retried. Follow-up state reads include the host session and reject responses from another session. Tests use an isolated HTTP responder and a deliberately blocked command queue.

Smart Playlist filters now read canonical Rust preparation extensions as flattened top-level fields, retaining compatibility with older nested phone drafts. Genre, artist, codec properties, energy profile and other analysis filters therefore work on the actual synced document format. Android still runs no analyzer and does not publish offline edits just because it reconnects. Explicit offline edit review/publishing remains outstanding.


The 0.2.3 Remote reconnect path also invalidates queued commands immediately on connection loss, before waiting to reconnect. Temporary network failures retry automatically with 3–30 second backoff; password/incompatible API errors stop automatic attempts for manual correction. Four focused Remote tests cover queued work on disconnect/loss, repeated refused handshakes and restarted-host state. Together with nine Smart Playlist checks, 13 tests pass on Pixel 7. The app is left installed after verification.


## Android 0.2.4 preview — simpler phone interface

- Persistent Player / Library / DJ / Cast / Remote tabs. More opens a full app chooser containing Lab and Settings. Existing themes and local files are retained; the shared chrome uses neutral grey surfaces, readable text and amber selection, with amber A / cyan B accents.
- Player keeps four transport buttons and Equalizer / Cast / Options. Empty queues offer Add music. Unavailable transport is disabled. Portrait keeps the queue below; short landscape windows put the player and queue side by side. Placeholder artwork is a scalable music-note vector.
- Library separates searchable Songs from Playlists, with one Add music entry. Playlist creation, Smart Playlist sources, genres, all saved-analysis filters, preview and editing remain available. Automatic energy hides its manual slider until needed.
- Phone DJ uses one Auto Mix switch. Turning it on without a queue opens the playlist chooser; choosing a nonempty list can start the mix. Turning it off cancels the automatic fade without stopping manual deck audio. Auto Crossfade prepares the next queued song before fading, waits for playback readiness, and abandons a stalled incoming track after 15 seconds while retaining the outgoing deck. Crossfade timing stepper, hot cues, jog seeking, sync and loops remain. Sampler / Cue Decks are grouped behind a labelled expander. A/B selection also sets and remembers the DJ browser's load destination.
- Remote puts discovery first, manual connection settings behind an expander and less-used mixer/sampler tools under More DJ controls. Host password selection expands the connection fields. The existing generation/session safety and reconnect backoff remain.
- Cast starts with nearby station discovery, optional Internet radio search and Offline playlists. Connected listening/Google Cast actions appear after connection; host startup/broadcast/speaker settings live under Station controls. Start radio & listen now explicitly selects the chosen host playlist before opening its stream. It controls the shared station, not a private independent phone stream. Google Cast's picker uses the same palette and insets, and accepts HTTP/HTTPS station streams; local-phone DJ broadcasting remains outside this update.
- Startup does not request a generic notification permission before the user can explore. Android's media-session notification exemption is documented by [Android Developers](https://developer.android.com/develop/ui/views/notifications/notification-permission#exemptions); notification settings remain available in Settings.

No analysis engine or background NAS editing was added. These screens use synced preparation and local overrides. Reverse scratching, full desktop DJ DSP/stems, offline-edit publishing and physical receiver acceptance remain separate work; controls do not pretend those features are implemented.


0.2.4 verification: build and lint pass; 22 Pixel 7 checks cover the new navigation/portrait-landscape bounds, advanced-control access, Smart Playlist data and filters, Remote reconnect/session safety and discovery, real silent DJ handoffs/staging/recreation, square pads in all three themes, authenticated playlist selection, and Google Cast picker initialization. Additional final-device checks verify screen-off Media3 playback with notification permission denied and recheck the primary layouts. Physical speaker playback was not repeated by this UI pass. APK names retain version/build/commit, and installation is in place without uninstall cleanup.
