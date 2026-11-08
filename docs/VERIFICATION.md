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

Visual pass source: `17457efb0effc901370cfeaf85abbf6ea8a79930`. [Windows/Linux CI](https://github.com/gitwide-cloud/amp-verse/actions/runs/42484196602): **both passed**, including all three Linux UI captures and the extended Cast test. Platform and UI artifacts were uploaded; no release tag was published.

## Lab and Plugin Manager references

Added an offline Plugin Manager with real included build versions, component search/filtering, module open actions, Cast companion-file presence detection, manual download links and explicit external-editor connection/launch. The supplied icon is shown in its detail panel. No network install/update system or multichannel AMP Lab editor is claimed. The existing editor repository was only read; no editor code was copied or changed.

Local optimized build and strict linting passed. Captured and inspected `test-output/plugin-manager.png`, including the AMP Lab optional/unconnected state. The external-editor launch/file-picker flow has not been manually exercised. CI now also captures the manager with `AMP_SCREENSHOT_VIEW=addons`.

## Three selectable themes

Local workspace tests (including preference replacement/round-trip), strict linting and optimized builds passed. Captured and visually inspected all three native player themes and the Modern Light Library, DJ and Plugin Manager. All twelve native theme/view screenshots were generated successfully in `test-output/{classic,modern-light,modern-dark}-{player,library,dj,addons}.png`.

In the live Cast browser UI, selected all three options and captured each appearance. Modern Light remained selected after a reload, verifying browser preference persistence. The station remained stopped throughout appearance QA. Evidence: `test-output/cast-{classic,modern-light,modern-dark}.png`. The temporary QA server was stopped afterward. No Android APK or device installation is claimed. Windows/Linux CI now captures all twelve native combinations; its result is separate from local verification.

## Cast playback web layout

Rebuilt the Cast page around the supplied responsive web references, retaining three locally saved themes. The optimized Cast binary built and the real encoder/network smoke test passed: protected Start/Stop, two listeners, MP3 decode and prepared Auto DJ transitions. In browser QA, an unauthorized Start opened access settings; entering the temporary QA password started the real station. Listen connected and status reported playback on this device; local Pause left the station on air. Stop disconnected local playback and reset the station. All three themes were visually inspected; Modern Light persisted across reload. Evidence: `test-output/web-reference-{dark,light,classic}.png`. Narrow-screen stacking is implemented in CSS; a physical mobile-browser check is still outstanding. No Android APK was built. QA server stopped after verification.


## Built-in native Cast dashboard

The latest supplied dashboard reference applies to native software Cast, independently of the browser interface. Added the native viewport and top-bar/Plugin Manager entry points. Workspace strict lint and eight local tests passed, including a new actual-TCP Cast client test for real status/queue schemas, password denial, empty-library start rejection, authorized stop, malformed endpoints and server disconnect. Optimized workspace binaries built. Captured the actual native window connected to a temporary real server in all three themes: `test-output/native-cast-{classic,modern-light,modern-dark}.png`; visually inspected Classic and both modern appearances. The displayed queue is the real generated fixture track, not mock content. The station remained stopped for screenshots; native mouse activation has not been manually exercised. Existing real encoder/stream validation remains separate from this client test. CI now captures five native surfaces in each theme. QA server stopped after inspection.

## Simple browser scope

Removed the disabled playlist-source selector and advanced sync/Auto DJ panels from the web page. The browser now exposes only basic station status, local listening, basic broadcast Start/Stop, theme/access settings and a collapsed read-only queue without preparation fields. Advanced editing/control ownership is documented for desktop and Android; Android remains unbuilt. The release binary built, actual browser status/theme rendering was verified, and the real Cast encoder/network smoke test passed. Screenshot: `test-output/simple-web.png`. The temporary QA server was stopped.

## First Android build / Pixel connection

Added a separate Android app in this repository: native compact Player/Library, locally copied SAF imports, atomic saved queue, real transport/seek, three locally saved themes, sequential playback/repeat and a Media3 foreground media session/service. Main APK and instrumentation APK build; Android lint passes with 0 errors and 9 warnings (dependency updates, backup/icon guidance and English-only text). Device instrumentation covers saved queue round-trip, decoder progress, sequential transition, pause and screen-off progress with generated silent fixtures, restoring the prior library afterward. That test has compiled but has NOT run yet.

The Pixel 7 initially appeared authorized as `Pixel_7` via USB, then disappeared before installation. Installation failed with device not found; subsequent device lists were empty. No APK was installed or uninstalled in this attempt, and no phone playback or screenshots are claimed. Reconnect is required to finish installation, real-device UI checks and the screen-off test. APK: `android/app/build/outputs/apk/debug/app-debug.apk` (12.5 MB approximately). The user requested the main app remain installed after testing; this is recorded in `ANDROID.md` and `ROADMAP.md`.

## Shared theme/product alignment and CI audit

Updated Classic to metallic grey chrome/beveled buttons with green displays across shared desktop skin, Android palette and simple browser theme. Screens/functions remain shared across the three themes. Documented desktop-only add-on management scope and MP3 Lab as the AMP Lab editor foundation; no second editor or changes to the separate MP3 Lab repository. Desktop strict lint/release build passed; Android APK/lint passed. Native Classic screenshot: `test-output/classic-metal-player.png`.

Verified actual GitHub results: Plugin Manager run 42485762891 and shared-theme run 42486445135 succeeded; native Cast run 42488047316 and simple-web run 42488203495 also succeeded. Android run 42488895196 failed in SDK setup requesting Google's removed `tools` package, before compiling the app. Changed setup to request only platform-tools, platform 35 and build-tools 35.0.0. Replacement CI result is pending; do not call it passed from local builds. Latest device check still showed no connected Pixel, so Android installation/device tests remain pending reconnection.

## Pixel 7 installation and actual playback test

After the USB connection returned, installed the current main APK and instrumentation APK successfully on the authorized Pixel 7. The device test passed: **OK (1 test)**, 6.602 seconds. It verified atomic saved queue round-trip, actual local decoder progress, sequential track transition, pause and continuing progress while the screen was off for 2.5 seconds. Generated silent WAV fixtures were removed and the prior library restored. This is a short functional test, not an endurance/audio-route matrix or human listening assessment. Main app remains installed as requested; opened its Player screen and captured `test-output/pixel-amp-verse.png`. SAF picker imports and full theme interaction still need dedicated UI coverage.

CI for source `7735de2` completed successfully: Android build/lint run 42489089495 and Windows/Linux desktop run 42489089347. These are verified results; the previous SDK setup failure is resolved.


## Android Settings and updater (0.1.1)

Added an internal native Settings activity reached from the topmost dropdown (Player, Library, Settings); moved theme selection and repeat-queue preference there. Added notification-settings shortcut, storage explanation and on-demand GitHub Releases version check plus browser download link. Shared Appearance palette covers Player/Library/Settings. No automatic APK installation or plugin loader is implied.

APK/test APK and lint pass. Updated main app installed on the Pixel 7 with its data retained. Playback regression test passed; two Settings/version tests passed separately (**OK (2 tests)**), including theme/repeat persistence over screen recreation and semantic version comparisons. The combined initial run stalled after the screen-off test left the device locked; adjusted Settings test setup for an awake test window and reran it successfully. Actual on-phone update check reported no public release/private-repository access, rather than claiming current/latest or an available APK. Visually inspected `test-output/pixel-settings.png`. Main app remains installed. No release update was installed from GitHub.
