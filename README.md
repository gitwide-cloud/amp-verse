# AMP⚡VERSE

Shared-library/server milestone: coordinated per-track analysis leases, persistent library/device IDs, conflict-aware metadata revisions and protected playlist saves now live in AMP Library Core. Cast can run the same incremental analyzer headlessly. See [coordination](docs/SHARED_LIBRARY_COORDINATION.md) and [server deployment/status](docs/SERVER_DEPLOYMENT.md). ARM32 binaries/images are candidates pending actual LAG validation; AUHUBNAS production remains gated. Google Cast and Android offline-sync UI are not implemented by this milestone.

An independent, modular audio project for Windows/Linux, with an integrated Android app planned. Inspired by compact classic players; not affiliated with Winamp. This repository is separate from MP3 Lab.

The supplied AMP Verse references now guide the drawn desktop chrome and embedded Cast UI: purple/silver Player and orange/cyan DJ decks, black/gold folder Library and Cast console. See [visual direction](docs/DESIGN.md). The Library uses folders and playlists; the player expands into an attached Library, with explicit magnetic merge and detach controls. Crates are excluded.

**Themes:** Classic, Modern Light and Modern Dark are selectable in the desktop top-bar dropdown (Appearance), and in Cast's navigation bar. Choices persist locally. See [theme settings](docs/THEMES.md).

## Companion and protected preparation

The desktop now exposes HTTP commands and passive WebSocket feedback for show control, with safe Load Only defaults, saved cue calls, preloading, fades, Duck/Restore, Manual DJ Lock and bounded retry deduplication. Library configures Smart Playlist rules; DJ performs a runtime copy, including prepared energy selection and progression. See [Companion setup and API](docs/COMPANION.md) and the [revised spec status](docs/SPEC_STATUS.md).

New preparation saves use a `.ampverse/tracks/` directory in the audio file's containing folder. Existing `song.ext.amp.json` records remain readable and unchanged; the portable record takes precedence. Reanalysis of approved/manual data stores a recommendation while preserving the existing values. Library shows exact before/after data, individual/bulk field decisions and audition, followed by explicit Save. Runtime DJ decisions do not save cues or rules.

## Library and Smart Lists

Expand Library below the compact player, or detach it into a native window. Move an AMP window against another window: the magnet button lights up and merges the sections when pressed. Each attached section has a Detach button. This works between the Rust Player, Library, DJ, Lab arrangement, Cast and Plugin Manager windows; the separately launched MP3 Lab editor stays independent. Merged groups scroll when needed, and layout currently resets on restart.

Library edits cue points, BPM/grid, key labels, ratings, energy, saved playback rate and portable text tags through **Edit cues / BPM / key / rating / energy**. Save writes `.ampverse/tracks/song.ext.json` beside the original audio; original audio and embedded tags are unchanged. Playback rate applies when the song is loaded and changes pitch; key edits correct metadata and do not transpose audio.

Build a playlist with the Mix checkboxes or **Add folder to playlist**. **Smart List · order for mixing** keeps the first song as anchor and orders the selected songs using prepared harmonic codes, tempo and energy. **Find similar songs** adds up to 20 analyzed library neighbors within ±6% BPM and ±0.2 energy, favoring compatible harmonic steps. Review, reorder and Save as a regular playlist. Missing metadata is reported; unknown candidates are excluded from expansion. Energy is a relative audio-level estimate or manual value, not a calibrated perceptual measurement. Recommendations do not guarantee a seamless mix. See [Library and Smart Lists](docs/LIBRARY.md).

## Runnable milestone 0.1

- **AMP Player:** native Rust/egui compact player, play/pause, stop/next, live master volume, an attached or detached folder Library, Shuffle, Repeat One, EQ and JSON playlists. Desktop playback uses the shared CPAL mixer with background FFmpeg decoding.
- **AMP Library:** shared Rust folder scanner, deterministic file tree, supported audio extensions, playlist persistence, validated prepared cue/BPM/beatgrid/stem-reference sidecars, root-confined file resolution. No crates.
- **AMP DJ:** shared-stream two-deck mixer, prepared cues, play/pause, pitch/speed, channel faders, manual and one-touch equal-power crossfade, live meters and scrolling source waveforms with cue markers, a saved nine-pad sound-effect sampler, captured loops, three-band EQ and low-pass FX. SYNC matches prepared BPM within the pitch range; beat-phase sync, key lock, live stems and broader effects remain unbuilt.
- **AMP Cast:** independent headless Rust server, embedded web UI, station launch/stop, sequential looping Auto DJ, one real-time MP3 encoder shared by browser listeners, optional simple admin password. Plays while the desktop app is closed.
- **Plugin Manager:** searchable offline component catalog, real included build versions, companion-binary detection and module launch actions. Open it from the top-bar dropdown.
- **AMP Lab:** manual links to MP3 Lab Releases and an explicit connection to a separately installed editor. Pick its executable in Plugin Manager, or set `AMP_LAB_EXE` to its full path before launch. Picker connections last for the session. **Open editor** launches it separately, without file handoff or modifying MP3 Lab. No editor code copied, automatic downloader, installer or native plugin loader yet.

**Not built yet:** Android prepared cues/stems, folder-tree import and LAN remote control; desktop/Cast LAN discovery and Android remote-control client; metadata tag extraction; persisted queues/session recovery; Cast local soundcard output, scheduling and station playlist selection; advanced DJ effects/routing; automatic updates or add-on installation. See [architecture](docs/ARCHITECTURE.md) and [roadmap](docs/ROADMAP.md). The UI reports prototype status; it does not display fictitious waveforms, meters or listener counts.

## Run from source

Requires stable Rust (edition 2024), FFmpeg with `ffmpeg` and `ffplay` on PATH, and a desktop OpenGL session for Player. These initial binaries **do not bundle audio tools**. Install FFmpeg separately. Linux build: `sudo apt install build-essential pkg-config libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev libgl1-mesa-dev libasound2-dev ffmpeg`.

```sh
cargo run -p amp-player -- /path/to/music
cargo run -p amp-cast -- /path/to/music
```

Windows PowerShell example:

```powershell
cargo run -p amp-player -- 'D:\Music'
$env:AMP_CAST_BIND = '0.0.0.0:8787' # opt in to LAN access
$env:AMP_CAST_PASSWORD = 'your-local-password' # optional, never stored in source
cargo run -p amp-cast -- 'D:\Music'
```

Open [Cast on this computer](http://127.0.0.1:8787). On another LAN device, use the server's IP and port 8787. Start the station, then click **Listen here**. The browser's audio controls manage listener volume. Cast scans its root at startup and loops all indexed tracks in folder order; restart after adding files. No internet is required after dependencies and audio tools are installed.

Cast defaults to loopback. A LAN bind exposes public listening and status; the optional password protects start/stop only. HTTP is for a trusted LAN: passwords are sent in a header over HTTP. For untrusted networks use a TLS reverse proxy and access controls; do not expose the bare server to the internet. No CORS is enabled, and browser cross-site control requests are rejected. Public status exposes track titles, never absolute paths. FFmpeg errors are shown in station status. Stop kills the encoder; slow listeners are disconnected rather than buffered indefinitely. Browser buffering can make stop audible later.

## Preparation and playlists

Alongside `song.flac`, optionally save `song.flac.amp.json`:

```json
{"cue_in": 2.0, "cue_out": 45.5, "bpm": 120.0, "beat_offset": 0.25, "stems": []}
```

Times are seconds. Only cues affect playback in this milestone; other fields are storage contracts. Malformed sidecars cause a visible scan error instead of silently losing preparation data. Cues are not yet checked against decoded track duration.

Playlist files use relative paths inside the configured music root:

```json
{"version":1,"name":"Evening","tracks":["Albums/song.flac"]}
```

Use the library window to add selected songs, save or load a playlist. Enable Auto DJ in AMP DJ to consume that queue on deck 1. Desktop decks use separate FFplay processes and the system default output; volume applies on the next play. Stop does not pause/resume. Queue edits are in memory until explicitly saved.

## Verify and build

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --release --locked
python tools/smoke_cast.py target/release/amp-cast
```

On Windows use `target/release/amp-cast.exe`. The smoke test requires FFmpeg and creates its own temporary audio. CI runs unit/API tests and release builds on Windows/Linux, real streaming/decode tests on Linux, and captures the actual Player window under Xvfb. Workflow artifacts contain both binaries and documentation. Tagging `v*` publishes a prerelease only after both platform jobs pass, with ZIPs and SHA-256 checksums. No release is implied by an untagged push. Linux artifact executables may need `chmod +x` after extraction.

GitHub Releases are the intended distribution channel for updates and optional add-ons. Downloads and installation remain manual in 0.1. Existing MP3 Lab releases remain independent.

## Licensing and provenance

MIT for AMP Verse code. MP3 Lab was inspected read-only for editor architecture, asynchronous jobs, subprocess cleanup and visual direction; see [reuse review](docs/MP3_LAB_REVIEW.md). No existing repositories are changed or runtime-coupled. Dependencies and external audio tools retain their own licenses; see [notices](THIRD_PARTY_NOTICES.md).


### Native AMP Cast dashboard

In AMP Verse, open the top-bar dropdown and choose **AMP Cast**. This opens a separate native window inside the desktop software, with Dashboard, Queue, Streaming and Settings sections. Enter the running server's `host:port` (default `127.0.0.1:8787`) and optional admin password, then **Connect / apply**. Start/Stop controls the independent server broadcast; the dashboard displays real track, cue, listener and station status. Closing the window does not stop the server. Passwords are kept in memory only. Start the `amp-cast` companion separately using the instructions above. Native soundcard listening and queue editing remain planned; the browser listener remains separately available.

The **web UI stays simple**: listen, see current status, start/stop broadcasting, and view the read-only queue. Real editing and advanced control belong to AMP Verse desktop and the Android app (Android remains in development).


### Android preview

`android/` now builds a compact integrated app (`cloud.gitwide.ampverse`) with Player/Library, three saved themes, file-picker imports copied to private local storage, a saved queue, seek/pause/previous/next, sequential queue playback/repeat and a Media3 foreground media service. See [Android build and device verification](docs/ANDROID.md). This is a first preview, not the complete Android milestone. No desktop plugins are loaded on Android.


Android **AMP Lab** is now the integrated MP3 Lab editor foundation: open audio, waveform/trim/fades/gain/normalize, original/edited preview, 256 kbps MP3 export and session restore. Open it from the top module dropdown. [Integration and source provenance](third-party/mp3-lab/PROVENANCE.md) includes upstream licensing and the corresponding codec source bundle. Desktop still uses the separately installed editor connection.


### Desktop DJ and Lab arrangement

AMP DJ has two deck platters and an embedded searchable library. Choose a music folder (including a mounted NAS folder), select Load A or Load B, then Play / Cue. Each deck uses the track's prepared cue IN/OUT. Live manual/auto crossfade, pitch, prepared-BPM tempo matching, EQ and low-pass FX are now implemented; see [DJ mixer](docs/DJ_MIXER.md). The chosen library folder is remembered for the next launch.

AMP Lab opens a mix arrangement with one lane per song. Add tracks from the scanned library, set timeline starts, cue IN/OUT and gain, then export one combined 320 kbps MP3 or individual trimmed MP3 files. Overlapping lanes mix together with a final limiter. Save/load arrangements as JSON. The existing installed MP3 Lab opens separately for waveform editing; arrangement bars indicate clip positions rather than audio waveforms. Arrangement fades, audition transport and automatic beat alignment are not implemented yet.

Cue metadata is read from adjacent AMP preparation files, such as `song.mp3.amp.json`:

```json
{"cue_in": 8.0, "cue_out": 188.0, "bpm": 128.0, "beat_offset": 0.0, "stems": []}
```

Times are seconds in the original file. This is AMP sidecar metadata; arbitrary DJ vendor cue tags embedded in ID3 are not imported yet. Without cue OUT, DJ plays to the end; Lab probes duration when adding a track.

Windows local installation after a release build: `./tools/install-desktop.ps1`. Optional `-AudioTools` points to an existing audio runtime directory containing ffmpeg/ffplay/ffprobe and its notices. Installation uses LocalAppData/Programs/AMP Verse and creates Desktop and Start menu shortcuts, without changing other applications. Installed audio tools are resolved beside AMP Verse; development can use PATH.


Desktop magnetic windows: drag the title rail beside the AMP VERSE dropdown to dock nearby native AMP windows. Attached windows move as a group; hold Shift while starting a drag to pull one free. Windows and X11 provide the required absolute positions; Wayland uses its compositor drag. The external MP3 Lab editor runs separately.


Desktop preparation is now an implemented first milestone: select a track in Library and choose **Prepare selected / cues...**, or use **Analyze library and save metadata**. The engine writes versioned key/BPM/grid/cue metadata beside the audio, preserves manual edits and provides DJ cue buttons. See [portable preparation](docs/PREPARATION.md) for the schema and accuracy/platform limits. Android sidecar consumption is still pending.
