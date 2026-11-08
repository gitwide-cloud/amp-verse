# AMP⚡VERSE

An independent, modular audio project for Windows/Linux, with an integrated Android app planned. Inspired by compact classic players; not affiliated with Winamp. This repository is separate from MP3 Lab.

The supplied AMP Verse references now guide the drawn desktop chrome and embedded Cast UI: purple/silver Player and orange/cyan DJ decks, black/gold folder Library and Cast console. See [visual direction](docs/DESIGN.md). The Library uses folders and playlists; older mockups' crates and integrated player strip are not included.

## Runnable milestone 0.1

- **AMP Player:** native Rust/egui compact player, Play/Stop/Next, next-play volume, a separate folder-library window and JSON playlists. FFplay performs local playback.
- **AMP Library:** shared Rust folder scanner, deterministic file tree, supported audio extensions, playlist persistence, validated prepared cue/BPM/beatgrid/stem-reference sidecars, root-confined file resolution. No crates.
- **AMP DJ prototype:** two independently playable decks, prepared cue IN/OUT playback, sequential Auto DJ from the queued playlist. No crossfade, beat sync, waveform editor, live stems or mixer yet.
- **AMP Cast:** independent headless Rust server, embedded web UI, station launch/stop, sequential looping Auto DJ, one real-time MP3 encoder shared by browser listeners, optional simple admin password. Plays while the desktop app is closed.
- **AMP Lab:** download link to existing MP3 Lab Releases. No editor code copied, installer, automatic downloader or plugin loader yet.

**Not built yet:** Android application; desktop/Cast LAN discovery and remote-control client; metadata tag extraction; local cue-editing UI; persisted queues/session recovery; Cast local soundcard output, scheduling and station playlist selection; native in-process audio engine; advanced DJ controls; automatic updates or add-on installation. See [architecture](docs/ARCHITECTURE.md) and [roadmap](docs/ROADMAP.md). The UI reports prototype status; it does not display fictitious waveforms, meters or listener counts.

## Run from source

Requires stable Rust (edition 2024), FFmpeg with `ffmpeg` and `ffplay` on PATH, and a desktop OpenGL session for Player. These initial binaries **do not bundle audio tools**. Install FFmpeg separately. Linux build: `sudo apt install build-essential pkg-config libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev libgl1-mesa-dev ffmpeg`.

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

Open [Cast on this computer](http://127.0.0.1:8787). On another LAN device, use the server's IP and port 8787. Start the station, then click **Connect listener**. The browser's audio controls manage listener volume. Cast scans its root at startup and loops all indexed tracks in folder order; restart after adding files. No internet is required after dependencies and audio tools are installed.

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
