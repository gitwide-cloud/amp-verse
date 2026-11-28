# AMP Verse request follow-through — November 28, 2026

Baseline: published RC11, canonical source `8dfb0e87c8caf60f276bad78f084a6e228bc86ab`. Historical chat completion messages are leads, not acceptance evidence. This list distinguishes implementation from device/listening verification.

## Confirmed gaps repaired for RC12

- Duplicate normal-playlist cards: `Browser::refresh_sync_saved` and `Player::accept_scan` compare literal paths. The same Windows playlist received as ordinary UNC and verbatim UNC produces two cards. Route scan, sync, Save and import through one lexical saved-playlist identity; preserve different files, song ordering, duplicates within a playlist, and revision protection. Never delete playlist files to fix display duplication.
- Automatic recovery: check only saved deck and cue inputs on a background worker, independently of the full library refresh. Restore paused only while playback and Auto Mix are idle. A slow storage check shows a waiting message after 15 seconds, retains one worker and the checkpoint, and disables duplicate restore attempts until that check returns. This deadline does not cancel operating-system network I/O. Actual source/output failures remain available under Details; never start audible playback automatically.

- Smart Playlist matching dispatch: removed the full track-metadata copy on the UI thread. The worker shares the existing immutable catalog snapshot. A 20,000-row production-code extraction benchmark measured 263.248 ms before and 0.102 ms after. This is not a total cold-screen latency claim.

Focused regression results before this optimization: 9 Browser, 9 Session and 19 Library tests passed, with one explicit native-audio recovery case pending the release gate.

## Requests present in the baseline

| Request | Implementation/evidence to retain | Acceptance still separate |
| --- | --- | --- |
| In-app desktop/server/Android updates | PR28; shared `amp-updater`, server updater, Android `AppUpdate`/`ReleaseCheck`; RC11 public catalogs and real update checks | OFFICECOMP is offline; first bootstrap there remains uninstalled |
| Analysis cancellation and failure count | `engine.rs`, preparation queue, organized analysis; RC9/10 regression gates | No new library-wide run required for this fix |
| Optional online lookup with analysis | `media_lookup.rs`; explicit opt-in and separate lookup cancellation | This fills reviewed catalog metadata; it does not imply every embedded MP3 tag has been corrected |
| Grouped/searchable genres | genre picker and RC6; included in RC11 | Ambiguous tags must not be guessed |
| Online holiday/season selections | `seasonal_lookup.rs`, cached MusicBrainz matches, smart filters | Online coverage is partial; preserve edition matching and offline behavior |
| Portable music/catalog export | `catalog_export.rs`/`archive.rs`; selected music and index, real RAR, 100 MB volume cap; RC10 fixture | RAR executable remains an optional external dependency |
| Plain playlist stem preparation | `playlist_stem_prep.rs`, `stems/playlist_prep.rs`, bounded prepared cache | Optional compressed preparation is deliberate and playlist-scoped; never resume library-wide WAV generation |
| Live stems, zero-knob stop, deck retention, Lab filters | live worker, six-deck controls, `lab_audio.rs`; Windows/Linux CI and prior local GPU checks | Hardware-specific 2/4/6 performance and listening quality are qualification results, not universal promises |
| Two jogs and six quick tracks | `deck_rack.rs`, `dj_browser.rs`; A left/B right, To Deck and quick transports | Physical controllers remain outside software-only acceptance |
| Player sources/queue and playlist builder | `organized_library.rs`, `playlist_builder.rs`, DJ browser | Keep checked-song actions explicit; suggestions never add automatically |
| Sortable table headings | `track_sort.rs`, table renderers; RC5 tests | Sorting the view must not silently reorder saved playback |
| Dislike/next, deck clear button | `deck_dislike.rs`, deck rack; RC4/5 tests | Original audio files remain unchanged |
| Waveform seeking/scrubbing and scratch control | deck waveform input and `mixer/vinyl_follow.rs` | Musical scratch feel still needs representative listening; do not substitute a canned scratch sample for the playing track |
| Spectrum/title palette | saved waveform palette, deck titles and accent controls | Preserve user's selected red/blue or yellow/neon choice through recovery |
| Compact Player/DJ and draggable header | `mini_player.rs`, drag/button hit-region tests; RC10 changes | Cold DJ entry/minimize responsiveness still needs a measured follow-up; do not claim no lag from compilation alone |
| Add-ons cards/status | `addons/catalog.rs`; only existing modules | Old installed modules without version receipts honestly show unknown comparison |
| Android audio-file opening and DJ pads | Android intent filters, Load path and separate cue/sampler banks; RC2+ builds | Pixel RC11 is installed; other phone still needs its own in-app update |
| Android Devices/Cast cards, discovery, host control | Android Cast/Remote panels and native device workflows | Physical receiver/network matrix and endurance remain separate |
| Server-owned Cast/playlists | server Google Cast, playlist-sync authority and hydration tests | OFFICECOMP must be awake for its real client/server and speaker checks |
| Normal/Smart playlist combinations | `show_editor.rs` playlist sources | Current sources are saved selections; subsequent source-list edits are not dynamic subscriptions |
| Lab recording and URL imports | recorder, YouTube and Suno modules | Newer Suno links may need official export fallback; microphone/loopback hardware differs |
| Empty folders | empty folder suppression and prior scoped cleanup | Do not repeat destructive music cleanup from historical requests |

## Data and quality work that must not be called complete

- Earlier MP3 audit reported 17 verification failures and held uncertain metadata. Review that report and actual files before any additional writes; preserve audio, artwork, cues and manual tags. A web title search alone is not exact recording identity.
- Hype mixing has drop/hook protection, lull exits and uncertain-analysis fallback. Representative EDM listening/section ground truth is still needed before claiming professional musical judgment.
- Overnight playback, physical receiver coverage and simultaneous hardware-qualified stem deck counts are not established by a green build.

Keep this list attached to the integration source. Update evidence after a fix ships; do not replace unresolved items with a queued-task claim.

## Follow-up recovery and Android filter corrections

RC12 owned-device startup exposed recovery waiting on the entire NAS refresh. The follow-up checks only saved deck inputs in the background and restores paused while playback is idle. A 15-second UI deadline reports slow storage without starting duplicate workers or claiming to cancel operating-system I/O. Portable catalog reads and writes take the same publication lock; writers publish immutable snapshots from local copies. A concurrent test exercised 32 guarded writes and 64 recovery reads without torn records or audio changes.

Focused Linux player recovery tests: 15 passed, 1 native-output test ignored. The shared 35-field Rust filter contract passed separately. Android Clean parity and a Java contract consumer are implemented; Android Gradle and native Windows startup acceptance remain pending. This source is not an installed or qualified release.
