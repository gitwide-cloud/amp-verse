# Milestone verification — November 8, 2026

Source under test: `7a7f92bf2918af4a5f27c7369f21e75c6ad22cb0` (Rust application code unchanged from `22d4a0cc34312310eddf825f3ca3065b665489d1`; the follow-up adds the Linux X11 runtime dependency and its setup instructions).

## Local Windows checks

- Stable Rust 1.99.1, x86_64 Windows GNU target, project-local toolchain/linker; no system Rust setup was required. Both optimized binaries built and ran. Local binaries are in `target/release` and are intentionally excluded from Git.
- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked -- --include-ignored` with `SDL_AUDIODRIVER=dummy`: **7 passed**, including real FFplay prepared-cue playback and stop/child cleanup. Linux additionally tests symlink escape and recursion prevention.
- `tools/smoke_cast.py`: passed against the optimized Windows server. Generated two audio fixtures, rejected an unauthenticated control request, launched the station, connected two simultaneous listeners to one broadcast, decoded captured MP3 bytes with FFmpeg, observed both Auto DJ tracks, stopped the station and rejected new playback while stopped.
- Ran the optimized Player with a generated audio folder and inspected its actual screenshot. Corrected missing control glyphs and the player layout. Library and DJ now use separate native viewports. Full interactive folder-picker, save/load and two-deck listening workflows still need manual desktop QA.
- Opened the real Cast UI in the browser, entered the temporary test password, clicked Launch and verified `ON AIR · Tone`; clicked Stop and verified `STATION STOPPED`. The temporary server was then stopped. Browser listener audio was validated at the encoder/network level, not by a human listening session.
- Local evidence: `test-output/player.png` and `test-output/cast.png` (ignored generated files). FFmpeg/FFplay were used read-only from the existing local MP3 Lab installation for these checks; AMP has no hardcoded MP3 Lab runtime dependency.
- `git diff --check`: passed. MP3 Lab remains clean at `d4a0f4a7c3fc8f990ca5b2f774ea63d4cabe4755`.

## CI

Final source run: [Build and verify AMP Verse](https://github.com/gitwide-cloud/amp-verse/actions/runs/42482889554): **Windows success; Linux success**. Both passed formatting, workspace tests, strict linting and optimized builds. Linux also passed the real FFplay test, two-listener Cast streaming/decode test and the Xvfb UI screenshot check. Platform build artifacts and a UI artifact were uploaded. The initial Linux lint failure was a Windows-only mutable variable declaration; it was corrected before this source commit. The first Linux screenshot attempt also exposed a missing `libxkbcommon-x11` runtime; CI and the README now install it explicitly. Superseded runs were cancelled.

The pipeline packages both executables and documentation as platform artifacts. A separate tag-gated job publishes prereleases only after both platform jobs succeed. No version tag or release was published during this milestone.

## Limits

No Android build, automatic updates/add-on installation, actual stem/FX processing, beat sync/crossfade, Cast soundcard routing, discovery, remote-control client, long-running endurance test, or complete human listening/device matrix is claimed. Cast transitions launch a new encoder per track and may have gaps. Desktop playback uses the default sound device and requires external FFplay; Cast requires external FFmpeg. No bundled audio runtime or installer is provided.

## Visual-reference follow-up

After the six reference images were supplied, the native Player/Library/DJ and Cast browser console were restyled. Library gained folder filtering, a search field, preparation/file-type columns and playlist item removal. Its save action is disabled for empty playlists. No external skin images or dependencies were added. Cast now exposes a read-only title/cue playlist endpoint and highlights the real broadcasting index, clearing it on stop/failure.

Local strict linting and all seven tests passed; the optimized binaries built. The extended real Cast smoke test passed, including public playlist schema/cue data, absence of filesystem paths, active index and index reset on stop. Captured and visually inspected all three native surfaces. Authenticated browser Launch and Stop still updated actual Cast state; the live playlist highlighted the broadcasting track. Evidence is in `test-output/themed-{player,library,dj,cast}.png`. The test server was stopped after browser QA. CI now captures each native surface independently using `AMP_SCREENSHOT_VIEW`.
