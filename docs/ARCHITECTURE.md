# AMP Verse architecture


## Desktop library and Android Remote ownership (current direction)

Desktop AMP Verse owns the music libraries, folder roots and playlists. Android Remote connects to that desktop/server, browses its songs/playlists and streams selected content to the phone. From that remote source, the user can explicitly Save offline; that downloaded collection belongs to the radio portion. A phone-folder import is a separate local-player source, not the definition of the Cast offline library.

Target flow: desktop adds library → Android Remote connects → browse songs/playlists → listen on phone → Save offline for Cast. Downloading a song or playlist must copy the actual audio and portable preparation to a managed radio cache, persist membership/source identity and report completion only once playback is available without the desktop. Offline radio items must remain usable when the source is unavailable; refresh must preserve local cue overrides. Do not cache a live station stream and label it as downloaded playlist songs. No desktop playback command should be sent when the user chooses to listen on the phone.

Current limitations: the Android build supports independent phone-file imports and direct radio-stream URLs. Desktop library browsing/streaming APIs, Remote browsing and Save offline from that source are not implemented. Retain existing imported phone audio; do not silently move, delete or relabel it as Cast downloads. Direct phone-folder playback remains available as a separate local-player source, as confirmed by the user.


## Boundaries implemented now

```text
amp-player (egui shell + Library/DJ windows)
    └── amp-core (folder tree / playlists / preparation / FFplay Deck)
amp-cast (Axum HTTP + embedded web UI + FFmpeg station broadcast)
    └── amp-core (same library and preparation contracts)
```

The Cargo workspace builds two independent executables. Cast has no GUI dependency. Core has no network or UI dependency and contains reusable media identity and preparation contracts. Audio commands receive arguments directly, never a shell command assembled from music filenames. A root-relative path must canonicalize inside the configured root; scans skip symlinks. Music roots are trusted local/NAS mounts; concurrent hostile filesystem replacement is outside the prototype threat model.

The desktop's scanner runs off the UI thread. Playlist entries reference library tracks rather than crates. Player controls remain in the compact main surface; Library has its own native viewport with folders and playlists, DJ another native viewport. Each module can be moved/resized independently of the small player. No web runtime is embedded in Player. Egui uses the OpenGL backend to reduce renderer dependencies. The current process-based audio adapter is an explicit first milestone, not a low-latency mixer.

Cast has one encoder task at a time. It traverses the prepared track list in deterministic order, decodes each track with input pacing, applies cue IN/OUT and emits 128 kbps stereo MP3. A bounded broadcast channel fans out to listeners; lagged clients disconnect. Each new track launches FFmpeg, so transitions may have gaps. There is no sample-accurate mixing, seamless stream guarantee, or beatmatching. Start is idempotent, Stop aborts the worker and kills its child via kill-on-drop. Public status is read-only; admin writes require the optional header password and same-origin browser requests. Listener playback is independent of local desktop decks.

## Target modules, not completed features

- **AMP Player:** always-small native shell, transport and module launcher. Windows/Linux first.
- **AMP Library:** shared audio engine, metadata index, folders/file trees, playlists, network locations with local cache, prepared analysis and manual cue overrides. Replace process decks behind a stable engine interface with an in-process decoder/mixer when low latency, crossfades and device routing are added. Consider Symphonia/CPAL after format/device testing; no commitment to untested codec support.
- **AMP Lab:** optional independently installed editor from GitHub Releases. Launch as a separate process with an explicit media/project handoff; preserve MP3 Lab licensing and provenance if code is ported. Do not dynamically load arbitrary native DLLs into the player. A versioned add-on manifest will declare compatible versions, asset digest, target OS and explicit install action. Verify checksums/provenance before any future installer executes downloaded code.
- **AMP DJ:** expandable deck count, prepared Auto DJ, cues and beatgrid overlays, then stems, effects, MIDI/OSC and Companion. Separate audio graph from controls. Controls must reflect engine state; do not animate fake signals.
- **AMP Cast:** independent headless service with browser administration, station lifecycle, listener endpoints and Auto DJ; later explicit output-device routing and scheduled playlists. A desktop controller is optional. Bind/discovery settings and credentials are separate from station preparation. Dual-path redundancy is an optional later Cast feature; there is no NetFuse or network-bonding module.

## Android target

Android stays **one compact integrated application**, not desktop plugins. Use a foreground media playback service and platform media session for screen-off/background operation; local storage/SAF imports and a local database hold downloaded songs and portable preparation. Only songs saved on the device are guaranteed offline. NAS/server preparation supplies stems, cues and beatgrid data; heavy stem analysis stays off-device. Device-local cue overrides take precedence over downloaded preparation and survive refreshes. Simplified Auto DJ uses locally cached songs and prepared data, with no promise of desktop FX parity.

LAN discovery should advertise a versioned AMP service with role and port (mDNS/DNS-SD planned). Android provides simple-password remote control of desktop/Cast. Discovery is a convenience, not authentication; manual IP entry must work when discovery fails. Offline playback must never require a station or account. An Android local-player preview now exists; discovery and Remote library protocol are still unimplemented.

## Data and evolution

Current sidecars store floating-point seconds, optional BPM/beat offset, and reserved stem references. Stem paths are not consumed in 0.1. Playlist schema version 1 stores a name and ordered relative paths; duplicates are valid. A later preparation envelope needs explicit schema/version, media fingerprint, analysis provenance, cue collections and separate manual-override records. Do not break existing playlists without a migration. Persist settings atomically before claiming crash recovery.

Network-first means NAS/server catalog, preparation sharing and remote control are first-class directions; it never means mandatory online startup. A mounted NAS root works now as ordinary filesystem access. Native network catalog/cache/sync is future work. Split optional crates so core and Cast remain small. Measure release size/startup/RSS before claiming a numerical footprint target.

GitHub Releases distribute platform builds and future add-ons. Release gating requires both platforms to build and tests to pass. Current ZIPs require external FFmpeg and use manual update installation. No self-updater or Android APK exists in 0.1.


## Native Cast control surface

`amp-player/src/cast.rs` supplies an egui viewport, separate from the browser UI and independent headless service. A dedicated worker owns a minimal ureq HTTP client and serializes status/queue polling and Start/Stop requests; UI/audio threads do not perform network I/O. Requests have a three-second timeout, do not redirect, and bypass ambient proxies for direct trusted-LAN access. Passwords are kept only in worker/UI memory and are sent solely on explicit control requests. This milestone accepts HTTP `host:port` endpoints. Disconnects invalidate displayed live state and disable broadcast controls; reconnect attempts continue every two seconds. The dashboard never owns the server or encoder lifetime.

Native tests use the actual Cast TCP router to verify JSON/status/queue compatibility, rejected passwords, empty-folder control errors, authorized stop and server disconnects. `AMP_CAST_ADDRESS=host:port` sets the initial address; `AMP_CAST_AUTOCONNECT=1` explicitly opts into connection on startup (no saved password). `AMP_SCREENSHOT_VIEW=cast` renders the actual dashboard for QA. The three desktop themes apply to this viewport, independently from browser preferences.


## Interface ownership

The web UI is intentionally basic: live station/track status, listening on this device, basic broadcast Start/Stop, a collapsed read-only queue and local theme selection. It is not the full Cast management dashboard. Playlist creation/reordering, library management, cue editing, Auto DJ rules/mixing, schedules, routing and detailed station controls belong in AMP Verse desktop and the integrated Android app as implemented. The server owns execution and prepared state so it remains independent of both clients. Android is still unbuilt; this ownership does not imply its controls already exist. Do not mirror advanced desktop mockups or placeholder future-feature panels into the simple browser page.


## Android preview implementation

The integrated Android application lives under `android/`, with its own package identity and no changes to MP3 Lab. Native Android views keep the initial shell small; Media3 ExoPlayer/MediaSessionService owns audio focus, noisy-route handling, media notifications and service playback. The activity connects by MediaController and releases its controller on stop without owning the audio engine lifetime. Imports use the system document picker and copy selected audio to private app storage on a worker thread, so later playback is local and independent of document-provider/NAS availability. `LibraryStore` atomically saves a relative-file queue. Themes use the same Classic/Modern Light/Modern Dark IDs in local preferences.

This preview has sequential queue playback and optional repeat; prepared cue overrides, beatgrid/stems, folder tree, named/reordered playlists, simplified cue-aware Auto DJ and LAN discovery/remote control remain unbuilt. No internet/storage-wide permissions or desktop plugin loader are included. The app currently targets API 35 and supports API 28+ (Android 9+). Playback duration and seek come from the actual player, not mock values.


## Current product decisions

- Application-wide themes share screens and behavior: Classic is metallic grey with beveled controls and green displays; Modern Light is bright/minimal; Modern Dark is charcoal with subtle accents. Earlier purple-first references are superseded by this Classic direction.
- The browser Cast surface stays minimal. Its Open App / Downloads action opens `https://github.com/gitwide-cloud/amp-verse/releases`.
- Desktop Plugin Manager is the home for downloading/managing add-ons. Current implementation is an offline catalog, manual download links and external-editor connection; automatic download/install/update management is still unbuilt. Android has an integrated feature set with no plugin loader.
- AMP Lab must extend/reuse the existing MP3 Lab editor foundation. Do not create a second independent editor implementation. Current integration launches an explicitly selected MP3 Lab installation; deeper editor integration/branding is future work. This does not authorize modifying the separate mp3-lab repository in this AMP Verse task.
- Use the Pixel 7 for real-device Android development/testing and leave AMP Verse installed once installation succeeds. A reported commit/local pass is not proof that CI or device tests passed; report actual results separately.


## Android audio processing

PlaybackService uses a custom DefaultAudioSink PCM chain with a five-band peaking EQ and a post-EQ spectral measurement tap (AudioDsp). The tap does not modify samples while EQ is bypassed. A foreground SpectrumView reads immutable volatile level snapshots and decays stale frames. No Android Visualizer/microphone permission, downloaded DSP or simulated spectrum is used. Settings preferences update EQ coefficients in the audio thread without letting the UI call decoder methods. Native Radio temporarily loads an explicit HTTP(S) stream into the same media session, while LibraryStore remains the persistent local queue authority.


## AMP Lab Android editor foundation

Android AMP Lab now integrates MP3 Lab's existing Android Java editor and pinned FFmpegKit audio dependency. It runs as a native activity in the integrated AMP Verse app; it is not an Android plugin or second independent editor engine. `lab/MainActivity`, `RetroLayout`, `Retro` and `SessionStore` retain upstream implementation with package/resource/navigation/shared-palette adapters and isolated session storage. The desktop Plugin Manager's external editor connection is unchanged. Source is inspected/copied read-only from the MP3 Lab checkout; future editor improvements should rebase/share that foundation, not drift into a separately redesigned processing engine. Provenance, MIT attribution, bundled codec notices and the exact corresponding-source archive are preserved under `third-party/mp3-lab/` and included with Android CI artifacts.


Lab mix arrangements live in `amp-core::arrangement`: clips reference library-relative files plus timeline start, preparation cue range and gain. Export resolves every source within the library root, trims source ranges, resets timestamps, delays clips to timeline positions, mixes and limits, then encodes MP3 on a worker thread. Individual exports use the same renderer with zero timeline start. The desktop Lab shell adds arrangement editing beside the existing MP3 Lab waveform editor; it does not fork the waveform engine. DJ uses the scanned folder library directly and keeps multitrack arrangement out of the deck interface.


Portable preparation now has a concrete version-1 implementation in amp-core::preparation/analysis. Desktop Library and amp-prepare use the same decoder/analyzer and adjacent metadata writer. See PREPARATION.md for persisted fields, manual protection, concurrency limits and unfinished Android consumption.


Desktop Player/DJ now use a shared CPAL stream and bounded FFmpeg decoder workers rather than separate FFplay outputs. Sample-clock crossfades and playback controls live in apps/amp-player/src/mixer.rs. Cast remains an independent encoder/server. See DJ_MIXER.md for controls, buffering, DSP and verification limits.


## Shared curation and explicit module composition

`amp-core::smart` owns deterministic energy-anchored playlist ordering and bounded recommendation expansion. Inputs are portable Preparation records; there is no GUI dependency. Rating, energy, playback rate and library tag overrides use validated/forward-compatible sidecar extensions. Audio analysis seeds missing energy from measured RMS; it preserves existing values.

`amp-player::window_layout` owns an acyclic module-to-host graph. Native edge snapping remains in docking.rs. A lit magnet button explicitly changes ownership; the group renderer reuses each module's existing UI under one native host, suppressing child native controls. Detach restores a native viewport and carries descendants with it. Layout is session-only and stacked; external MP3 Lab stays separate.


## DJ sampler

`amp-player::sampler` owns the nine-pad UI, saved file assignments/levels and a serialized background FFmpeg loading queue. PCM buffers are bounded to 30 seconds / 12 MiB per pad. `mixer::Engine` owns the resident buffers and sends load/trigger/stop commands to the same CPAL callback used by the decks. The sampler uses nine independent one-shot cursors, per-pad and bank gain, then the shared master. Its bus bypasses the deck crossfader and deck EQ. Generation acknowledgements release replaced PCM allocations on the UI thread after callback ownership ends. Sampler settings are local device configuration, not portable song preparation metadata.


## Show-control and approval milestone

`amp-player::control` runs a bounded HTTP/WS gateway on a background Tokio runtime. Commands carry retry IDs, deadlines and per-command reply channels; the UI thread owns execution and publishes observed CPAL-backed state. Networking never owns or stops the local audio stream. The last 512 results are cached per gateway session. WS subscription and reconnection are passive. Default bind is loopback with explicit trusted-LAN opt-in.

`amp-player::performance` owns session-only Smart Playlist copies, prepared track/cue loading, incoming-buffer waits, automatic crossfades, repeat history, manual lock, volume duck/restore and progression that advances on completed transitions. The shared callback runs level ramps per sample. Deck load generations prevent feedback from announcing a requested track before the callback has installed it.

`amp-core::show` holds portable saved rules, IDs and selection logic. `amp-core::review` applies explicit field-level recommendation decisions. `amp-core::preparation` prefers `.ampverse/tracks` records and falls back to legacy adjacent sidecars; only explicit/automatic unconfirmed preparation writes use the new store. Runtime playback has no write path. Approved values are held unchanged when analysis creates a candidate. Manual hot-cue numbers and unknown per-cue extensions are preserved. Saved show-rule folder paths normalize across Windows/Linux separators.

Detailed implementation boundaries and unmet revised-spec items are recorded in SPEC_STATUS.md.
