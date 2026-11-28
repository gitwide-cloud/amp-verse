# AMP⚡VERSE

**Your music. Your playground.**

AMP⚡VERSE is an open-source music player and DJ playground built for music hobbyists, home labs, and anyone who loves having their own music collection.

Maybe you just want a simple player for the music on your computer. Maybe you want to build playlists, mix tracks like a DJ, or run your own little radio station from a home server. AMP⚡VERSE brings those possibilities together without making you subscribe to a music service or give up control of your files.

It takes inspiration from the music players many of us grew up with: **a classic, Winamp-inspired interface when you want the nostalgia, and clean modern light and dark themes when you don't.** It's a separate, independent project and isn't affiliated with Winamp.

> **Play your music. Mix your music. Take it with you.**

## 1.1.0 release candidate

This candidate aligns desktop, server, and Android at 1.1.0-rc.1. Test
packages are installed manually into existing installations. The in-app
updater offers stable releases. Android uses the existing app IDs and signing
identity.

Selecting a Smart Playlist for Auto Mix leaves current deck audio in place.
Turn Auto Mix on to load and play from the selected source. Phone-to-server
live audio sending and sustained stem tiers above two decks are still pending
release acceptance.

## What can you do with it?

| | |
| --- | --- |
| **🎵 AMP Player** | Just press Play. Listen to your collection, manage a queue, adjust the EQ, and enjoy a proper spectrum visualizer. Keep it compact or open the Library alongside it. |
| **📁 AMP Library** | Point the app at your music folders, browse your real folder tree, make playlists, and prepare tracks with BPM, key, energy, cues, and beatgrids. Your approved edits stay yours. |
| **🎚️ AMP DJ** | Mix on two full decks with jog wheels, hot cues, loops, EQ, effects, stems, samplers, and Auto DJ. Build a Smart Playlist and let the software help choose and blend tracks. |
| **✂️ AMP Lab** | Trim a track, make a sampler clip, edit fades and levels, or assemble a longer layered mix for MP3 export. |
| **📻 AMP Cast** | Share what you're playing live, host a playlist-based radio station on your network, and listen from another computer, browser, or supported Cast device. |
| **📱 AMP Verse for Android** | Take selected music and prepared playlist information offline, listen and mix on your phone, or use it as a remote for your desktop music setup. |

### Made for your home lab — but you don't need one

Start with one Windows or Linux computer and a folder of songs. That is enough to enjoy AMP Player.

If you've got a NAS, another computer, or a home server, you can take it further: keep music in one place, reuse preparation information, tune in to your own stations, and control supported players across the LAN. Your playlists can travel with you, and Android can save selected station playlists and their music for offline listening.

**No cloud account is required for ordinary local playback.** Your library is yours.

## Simple when you want it. More powerful when you need it.

- **Classic or modern:** Choose Classic, Modern Dark, or Modern Light. The desktop also supports compact and movable/attached windows.
- **Your folders, your playlists:** No crate system to learn. Browse folders, build regular playlists, or create Smart Playlists.
- **Auto DJ:** Choose a playlist and a style—**Hype**, **Steady**, or **Chill**—and let the engine use available music analysis to guide selection and transitions.
- **Hands-on DJing:** Use cues, loops, a crossfader, sampler pads, and prepared stem controls. You can work manually or let Auto DJ keep music flowing.
- **Music on the network:** AMP Cast supports live master-audio broadcasts and playlist-based listening. Use supported remote controls and Google Cast where available.
- **Keep your preparation:** Analyzed metadata and manual corrections are reusable; automatic analysis suggests improvements rather than silently overwriting approved edits.
- **Metadata during analysis (desktop):** In Library → Analyze Music, optionally enable **Look up missing metadata while analyzing**, then Start / resume analysis. MusicBrainz receives titles and artists, not audio. Lookup runs separately in batches of up to 500 music tracks; audiobooks and spoken recordings are excluded. Open **Tags & artwork** to review the correct edition and save missing fields/artwork into AMP Verse. Existing edits and embedded MP3 tags are preserved; mismatches can be discarded. Internet lookup can be stopped without stopping audio analysis.
- **Holiday and season matching (desktop):** Holiday/Season Smart Playlist filters can match public MusicBrainz catalogs against your library locally, including songs without seasonal file tags. No audio or library list is uploaded. Results are cached in the shared library; tags remain an offline fallback. Matching is conservative about artist/edition, and catalog coverage is shown rather than assumed complete.
- **Portable catalog export (desktop):** Library → Analyze Music → Export catalog selects folders, playlists, Smart Playlists, categories and filters. It archives selected original music plus a fresh portable index, saved cues and selected artwork into genuine RAR volumes of at most 100 MB. WinRAR/RAR must be installed separately. Temporary staging holds metadata rather than a second copy of the music; canceled or failed exports leave originals unchanged.


## Windows, Linux, and Android

**Windows & Linux** are the most feature-complete editions. AMP Player is the home base; DJ, Lab, Cast, and optional analysis/stem capabilities expand what it can do. Some add-on packaging and automatic installation workflows are still being refined.

**Android** is intentionally lighter. It includes a local player, playlists, prepared cue-aware Auto DJ, offline playlist downloads, an integrated audio editor, radio listening, Cast support, and network remote capabilities. It reads prepared metadata rather than running heavyweight music analysis or stem separation on the phone.

The desktop and Android editions do not yet have identical features. See [Android documentation](docs/ANDROID.md) for current capabilities and limitations.

## Get AMP⚡VERSE

**[Browse releases](https://github.com/gitwide-cloud/amp-verse/releases)** · **[Builds and tests](https://github.com/gitwide-cloud/amp-verse/actions)**

Download an available build for your platform from Releases. This is an actively developed project: check the release notes and workflow results before treating a beta as production-ready.

If you'd rather build from source, you'll need the Rust toolchain and the audio/runtime dependencies described in the project documentation.

```bash
cargo build --workspace --release --locked
cargo test --workspace --locked
```

On systems with the required audio tools and a working sound output, run the desktop player with a music directory:

```bash
cargo run -p amp-player -- /path/to/music
```

For the independent Cast service:

```bash
cargo run -p amp-cast -- /path/to/music
```

See [architecture](docs/ARCHITECTURE.md), [server setup](docs/SERVER_DEPLOYMENT.md), and [Android build notes](docs/ANDROID.md) for platform-specific setup details.

## What's still being built?

AMP⚡VERSE has real playback, editing, DJ, and networking code, but it is **not yet a finished professional DJ product**. Current work includes:

- Improving musical analysis, downbeat/phrase accuracy, and pitch-preserving tempo changes.
- Refining the DJ interface, session recovery, controller support, and responsive layouts.
- Hardening Android remote operation and cross-device offline-edit synchronization.
- Improving headless Smart Radio mixing, receiver compatibility, and long-running server operation.
- Polishing optional add-on downloads, updates, packaging, and releases.

Some features require specific audio hardware or have not yet been fully verified across devices. The current implementation and known limitations are described in the linked documents and release notes.

## Go deeper

- [AMP Library and music preparation](docs/LIBRARY.md)
- [Smart Playlists and preparation](docs/PORTABLE_SMART_PLAYLISTS.md)
- [DJ mixer and controls](docs/DJ_MIXER.md)
- [Companion and remote API](docs/COMPANION.md)
- [Shared library coordination](docs/SHARED_LIBRARY_COORDINATION.md)
- [Themes and visual design](docs/THEMES.md)
- [Current development status](docs/NOVEMBER_ROADMAP_STATUS.md)
- [Project architecture](docs/ARCHITECTURE.md)

## Open source and credits

AMP⚡VERSE is an independent, hobbyist-focused, open-source project. The project's code is MIT licensed; bundled tools, codecs, and other dependencies retain their own licenses. See [third-party notices](THIRD_PARTY_NOTICES.md).

AMP Lab builds on ideas and technology from the separate [MP3 Lab project](https://github.com/gitwide-cloud/mp3-lab). MP3 Lab remains its own project.

This is a project for people who miss the personality of classic music software and still enjoy discovering what they can build with their own music. **Listen casually, DJ a party, or turn your home lab into a radio station—it's your call.**

### Updates inside the apps

Desktop: Add-ons → Update. Server: its web window → Add-ons → Update server. Android: Update in the app; each separate add-on also has Update this add-on. Downloads are verified before replacing application files. Server updates restart only the existing server; music, playlists, pairing and settings stay in their existing locations. Android asks for its required installation approval.

RC builds follow newer RC releases and can promote to stable; beta builds can advance to RC or stable. Stable builds remain on the stable channel. Drafts and releases without the complete update manifest are ignored. Versions are compared numerically, including RC 9 → RC 10. This updater fix must first be applied once to older installations whose update code only understands stable releases.
