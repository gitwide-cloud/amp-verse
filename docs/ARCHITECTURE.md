# AMP Verse architecture

## Boundaries implemented now

```text
amp-player (egui shell + Library/DJ windows)
    └── amp-core (folder tree / playlists / preparation / FFplay Deck)
amp-cast (Axum HTTP + embedded web UI + FFmpeg station broadcast)
    └── amp-core (same library and preparation contracts)
```

The Cargo workspace builds two independent executables. Cast has no GUI dependency. Core has no network or UI dependency and contains reusable media identity and preparation contracts. Audio commands receive arguments directly, never a shell command assembled from music filenames. A root-relative path must canonicalize inside the configured root; scans skip symlinks. Music roots are trusted local/NAS mounts; concurrent hostile filesystem replacement is outside the prototype threat model.

The desktop's scanner runs off the UI thread. Playlist entries reference library tracks rather than crates. Player controls remain in the compact main surface; Library has its own egui window with folders and playlists, DJ another window. These are in-app floating windows, not separate OS-native windows yet. No web runtime is embedded in Player. Egui uses the OpenGL backend to reduce renderer dependencies. The current process-based audio adapter is an explicit first milestone, not a low-latency mixer.

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
