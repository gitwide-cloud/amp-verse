# AMP Verse browser server on Sager

The browser studio runs from the Linux `amp-cast` binary; its web assets are embedded in the application. The QNAP project remains the source of truth. Compilation, dependency cache, server state, and demonstration audio live on Sager VM100's HGST disk. No new NAS media share is required.

## Paths and network

- VM: `sager-dev` / `192.168.1.78` (Debian 13).
- Source: `/mnt/codex-builds/Software builds/amp-verse`.
- Isolated source copy: `/srv/sager-work/sager/projects/amp-verse-browser`.
- Cargo cache: `/srv/sager-work/sager/cache/cargo`.
- Build output: `/srv/sager-work/sager/cache/amp-verse-target`.
- Runtime: `/srv/sager-work/sager/services/amp-verse-browser`.
- Unit: `amp-verse-browser.service`.
- LAN address: `http://192.168.1.78:8790` (port checked free during setup).

The first installation uses a generated sine-wave demonstration folder. It does not access Jellyfin's VM101 mount or grant additional NAS permissions. There is no router forwarding or public listener configuration. Discovery starts disabled. Protect non-demo libraries with a privately configured `AMP_CAST_PASSWORD`.

## Build and validate

Install official Debian packages (Rust 1.96 from configured trixie-backports supports the current lockfile):

```sh
sudo apt-get install -y -t trixie-backports rustc cargo rustfmt
sudo apt-get install -y ffmpeg nodejs rsync curl libasound2-dev
sh '/mnt/codex-builds/Software builds/amp-verse/deploy/browser-server/build-linux.sh'
```

The script copies current source, including uncommitted changes, without writing into the QNAP checkout or copying `.git`, dependency caches, or compiled files. It then runs `cargo test --locked -p amp-cast` and builds the release executable. The script does not start or install a service. Node is available for JavaScript validation; a separate web bundler is not required for embedded static assets.

## Install after successful tests

Run as `sager` on VM100. Review the environment file before starting. Existing deployments should retain their environment and config directories.

```sh
runtime=/srv/sager-work/sager/services/amp-verse-browser
source='/mnt/codex-builds/Software builds/amp-verse'
mkdir -p "$runtime/bin" "$runtime/config" "$runtime/demo-music" "$runtime/xdg-config" "$runtime/cache"
install -m 755 /srv/sager-work/sager/cache/amp-verse-target/release/amp-cast "$runtime/bin/amp-cast"
# Initial installation only; do not overwrite a configured server.env.
test -e "$runtime/server.env" || install -m 600 "$source/deploy/browser-server/server.env.example" "$runtime/server.env"
ffmpeg -nostdin -n -f lavfi -i 'sine=frequency=440:duration=30' -c:a pcm_s16le "$runtime/demo-music/AMP-demo.wav"
sudo install -m 644 "$source/deploy/browser-server/amp-verse-browser.service" /etc/systemd/system/amp-verse-browser.service
sudo systemd-analyze verify /etc/systemd/system/amp-verse-browser.service
sudo systemctl daemon-reload
sudo systemctl enable --now amp-verse-browser.service
curl --fail --silent --output /dev/null http://192.168.1.78:8790/
systemctl --no-pager status amp-verse-browser.service
```

For updates, stop the service before replacing its executable, then start it after the verified binary is installed. Preserve the previous executable to enable rollback. Stop only this unit: `sudo systemctl stop amp-verse-browser.service`. Logs: `journalctl -u amp-verse-browser.service -n 100 --no-pager`.

`ProtectSystem=strict` permits writes only within the runtime directory. Library paths elsewhere are read-only to the service. Keep analysis and generated project data inside configured runtime paths. First deployment disables analysis to avoid unnecessary work on demo audio.

## Current browser coverage and deployment status

This is a browser-server preview. The initial demo deployment has now been validated against a real two-source NAS library: 13,963 tracks (4,405 main-library tracks plus 9,558 additional-source tracks) and nine saved Smart Playlists. Personal music and catalogs are not included in release packages.

The browser provides a local Player and DJ decks with manual tempo adjustment and crossfade, plus a nine-pad sampler. Server Lab supports MP3 export and saved track preparation. Smart Radio uses the shared core. Podcasts supports search, RSS feeds, subscriptions, and streamed playback.

Full parity with every native add-on is not complete. Advanced stems and controller workflows, the arrangement editor, URL downloading, and podcast offline-download/purge workflows have not been ported. There is no browser engine add-on installer. These native capabilities should not be described as available in this preview.

Linux verification completed during preparation: the complete amp-cast suite passed (22 library and one startup test; four environment-dependent tests ignored), two core podcast tests passed, and all ten focused studio tests passed including three API integration tests and a real FFmpeg waveform regression. The browser JavaScript behavior test passed on Node 20.19.2. Final assets were rebuilt and live browser QA verified playback, decks, saved playlists, Lab export, waveform, RSS podcast preview/search, and responsive layout. The persistent service is enabled and active at http://192.168.1.78:8790/; restart verification preserved the demonstration library and saved playlists. The real FFmpeg/network smoke test passed password checks, start/stop, two shared listeners, MP3 decode, and automatic track transitions. Analysis remains disabled. The exported six-second preview was verified as MP3 at 256 kbps (6.034 seconds including frame padding); the source WAV remains 30.000 seconds.
## Real libraries and portable source locations

A Linux server can reuse a Windows library without rewriting its portable source list. Set `AMP_CAST_MUSIC_ROOT` to the mounted main library. Optional `AMP_CAST_SOURCE_LOCATIONS` is a JSON object mapping persistent additional-library IDs to absolute Linux mount paths. Each override must match that library's saved identity; missing or mismatched mounts are rejected. Keep this setting in the server's private environment, not in the shared music catalog.

Music and existing analysis remain in their original NAS folders. The service can restrict source audio to read-only while granting its `ampdj` metadata directories access for shared publication locks and explicitly saved preparation/playlists. Do not enable analysis until storage permissions and resource policy are configured. All catalog updates retain the existing revision/conflict checks.

For systemd EnvironmentFile syntax, quote the JSON value with single quotes. Example (replace IDs and paths):

```
AMP_CAST_SOURCE_LOCATIONS='{"additional-library-id":"/mnt/media/extra-music"}'
```

Configure `AMP_CAST_PASSWORD` privately before connecting personal media. The browser Access panel accepts it for the current page session. Neither passwords, personal catalog snapshots nor music belong in release archives. This preview is intended for a trusted LAN; use an authenticated TLS reverse proxy before remote access. No internet forwarding is configured by the deployment unit.

## Real-library acceptance check (2026-11-14)

The Linux source-location regression test passed. From the exact committed source, the server suite passed 23 tests with four environment-dependent tests ignored; the browser JavaScript behavior check passed. Real-library verification covered authenticated pagination, both source audio streams decoded by FFmpeg, waveform generation, three-second MP3 export, rejected stale preparation writes, unchanged sampled original audio, and all nine saved Smart Playlists with nonempty selections. Browser QA confirmed actual Player playback advancing without a media error and an R&B Smart Playlist broadcasting to two HTTP listeners, including browser playback. The station advanced to another song automatically during verification. Test playback was stopped afterward.

The first full NAS metadata scan took about eight minutes. The resulting device-local startup snapshot is approximately 95 MB; the installed service became ready with all 13,963 tracks in approximately 1.1 seconds using that snapshot. This measures API readiness on this deployment, not a universal performance guarantee. Analysis stayed disabled.

Known limitations for follow-up: the current Refresh button reloads the in-memory catalog page rather than launching a new scan. Startup background scanning updates the saved snapshot for a subsequent restart; it does not hot-swap the running immutable library. Incremental scanning with explicit refresh and stable track identifiers is a priority for the next version. The browser provides only the capabilities listed above, not complete desktop/Android add-on parity.