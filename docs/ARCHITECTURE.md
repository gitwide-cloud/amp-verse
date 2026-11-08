# AMP Verse architecture

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

LAN discovery should advertise a versioned AMP service with role and port (mDNS/DNS-SD planned). Android provides simple-password remote control of desktop/Cast. Discovery is a convenience, not authentication; manual IP entry must work when discovery fails. Offline playback must never require a station or account. No Android app or discovery protocol is implemented yet.

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
