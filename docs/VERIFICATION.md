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


## Android compact Classic and six-module navigation (0.1.2)

Built main/test APKs and passed Android lint after adding shared Chrome drawables, compact native layouts and six named dropdown/module shortcuts. Actual playback code/service remains the same; no spectrum simulation or fabricated content was introduced. Pixel installation was attempted but failed because the previously connected device disappeared from USB. No new on-device UI/screenshot or playback-test pass is claimed for this build. The prior installed 0.1.1 app was not removed; 0.1.2 device validation requires reconnection. Prior on-device results remain tied to their earlier builds.

## Pixel 0.1.2 follow-up

USB connection returned. Installed 0.1.2 successfully (versionCode 3), opened the native app and checked the dropdown containing AMP Player, AMP Library, AMP DJ, AMP Remote, AMP Lab, AMP Cast and Settings. Selected Classic through Settings and visually inspected the actual silver/beveled layout with dark/green display. Captures: `test-output/pixel-012.png`, `pixel-012-menu.png`, `pixel-012-classic.png`. The actual playback regression test passed on this build: **OK (1 test)**, 7.002 seconds, including local queue/transition/pause and short screen-off progress. Test fixtures were restored/removed as before. Main app remains installed and Classic selected. Remote/Lab/Cast destinations remain explicitly labeled unbuilt previews, not working native integrations.


## Android folder scan (0.1.3)

Added system folder picker, recursive iterative SAF traversal on an import worker, audio filtering, source-URI duplicate skips and progress feedback. Individual-song selection remains available. Main/test APK builds and Android lint passed. Installed on the connected Pixel 10 Pro XL, retaining app data. A device test using a test-only DocumentsProvider passed (**OK (1 test)**): nested audio discovery, extension fallback, non-audio exclusion and traversal-cycle protection. The first test-provider attempt lacked Android's required MANAGE_DOCUMENTS protection; corrected the test-only manifest/permissions and reran successfully. Production requests only the user-chosen folder URI grant, not MANAGE_DOCUMENTS or broad storage access. Full end-to-end user-folder copying and duplicate-rescan UI still need manual coverage. No user music was selected by the agent during this test. Main app remains installed; Pixel 7 was not connected for this update.


## Android EQ, shuffle/repeat, radio and live spectrum (0.1.4)

Main/test APKs built; Android lint passed after explicitly opting into the pinned Media3 processing APIs. Installed 0.1.4 on the connected Pixel 10 Pro XL, retaining app data. Device DSP test passed (**OK (1 test)**, 0.631 s): PCM bypass/flat gain, actual 1 kHz attenuation from EQ, tone-band spectrum and zero output for silence. Loopback HTTP radio test passed (**OK (1 test)**, 2.975 s): actual network WAV decoding, progress and PCM spectrum tap through the service pipeline, using a generated fixture at muted volume and restoring the saved queue. Local playback/screen-off regression also passed (**OK (1 test)**, 6.694 s). No external radio station or FM hardware is claimed tested.

Visually inspected actual player with the new spectrum and controls on-device; `test-output/pixel-014-player.png` shows real saved phone audio and active spectrum, not reference/mock values. Screenshots remain local test evidence and are not committed. Shuffle/repeat use actual Media3 modes and preferences; full shuffled-order/endurance coverage remains outstanding. Main app remains installed; no other phone was connected for this update.


## Android source ownership clarification (0.1.5)

User confirmed direct phone-folder playback stays separate. Desktop AMP Verse owns shared libraries/playlists; Android Remote is the intended browsing/listen-on-phone path, with Save offline creating the radio collection. Added explicit source labels and Remote workflow explanation to Android Library/Settings. Existing phone imports and local playback remain intact; no entries were deleted or converted to radio downloads. Desktop browsing/streaming/Save offline are not claimed built. Android APK/lint passed. Installed this wording/source-navigation update on the connected Pixel 10 Pro XL; app data retained. No audio-engine or transport changes were made in this clarification.


## Integrated AMP Lab from MP3 Lab (0.1.6)

Located the existing `mp3-lab/dist/v0.2.2-preview.27/mp3-lab.apk` and matching editor source family. Reused its four Android editor files under MIT inside AMP Verse, with package/navigation/theme adapters and isolated session storage. Original MP3 Lab checkout remained clean/read-only. Added its pinned FFmpegKit audio runtime, bundled notices and unchanged corresponding-source archive with provenance/hashes; Android CI artifacts include that source bundle. ARM64 APK is about 33.9 MB.

APK/test APK build and Android lint passed. Installed 0.1.6 on the Pixel 7 and left installed. Adapted upstream AudioEngineTest passed (**OK (1 test)**, 3.662 s): real trim/fades/gain/normalize, MP3 bitrate/duration/decode, document export, preview and relative seek. Adapted EditingSessionTest passed (**OK (2 tests)**, 51.78 s): rotation/recreation/background session state, waveform zoom/pan/tap and millisecond nudge. Native Lab navigation was exercised from the module strip and the actual editor inspected in Classic; evidence `test-output/pixel-016-lab.png`. Modern palettes are adapted but have not received the same full manual Lab visual/device matrix. MP3 Lab's multichannel/stems and advanced DJ preparation remain unbuilt. Pixel 10 was not connected for this update.


### Android 0.1.7 compact Player (2026-11-08)

Rebuilt Player around the supplied compact Classic reference: a single AMP VERSE module dropdown, live PCM spectrum, artwork/details, seek/time, six transport buttons, four utility buttons and a scrolling queue. Classic queue text is green on black with a yellow current-item highlight. Removed the Player module tabs, redundant headings, extra action row and default explanatory footer. Settings and all six modules remain in the dropdown; the same layout uses the existing three application palettes. Artwork uses decoded media artwork when available, otherwise the app icon; unavailable metadata is not fabricated.

Debug APK and lint passed. Installed on the authorized Pixel 7 (31160DLH200078), preserving app data. PlaybackDeviceTest passed (1 test, 6.529 seconds), including sequential transition and screen-off playback. Final Classic empty-state layout inspected on-device; screenshot is local test-output/pixel-017-player.png. Only Pixel 7 was connected; the other phone was not updated.


### Desktop DJ library, Lab arrangement and local installation (2026-11-08)

Workspace unit/integration tests passed (10 tests; two audio-tool tests opt-in). Explicit real FFmpeg arrangement test passed: generated source audio, two overlapped clips, cue trim, 0.5-second timeline offset, output duration within 0.1 seconds, refusal to overwrite, and decoding an individual MP3. Clippy with warnings denied and release builds passed. The explicit prepared-deck audio test also passed with the silent SDL audio device (1.43 seconds). Actual installed DJ view captured in local test-output/desktop-dj-library.png. New Lab arrangement view uses real clip placements, without simulated waveforms.

Installed release Player and Cast binaries at C:/Users/pfgpe/AppData/Local/Programs/AMP Verse, with Desktop and Start menu shortcuts. Copied the audio tools and their notices/licenses from the already installed MP3 Lab runtime. Source mp3-lab repository remains unchanged. This is a local desktop preview install, not a newly published GitHub Release. CI was extended to capture Lab along with the other views; remote CI completion is not asserted here. Android was unchanged by this desktop milestone.


### Dropdown-only module navigation (2026-11-08)

Removed the redundant Android module tab strip on every main module screen, not just Player. All six module destinations plus Settings remain in the AMP VERSE dropdown; Android Lab already uses that dropdown. Removed desktop Player module shortcut buttons; the title dropdown remains the desktop module navigator. Android 0.1.8 assemble and lint passed.


### Android updater test release 0.1.9 (2026-11-08)

Version 0.1.9 (code 10) builds and passes lint. Prepared for GitHub Releases without installing over the phone's 0.1.8 (code 9), so its updater has a newer version to detect. Uses the existing local preview signing certificate. Published releases use plain vMAJOR.MINOR.PATCH tags and GitHub latest-release eligibility because the installed updater queries /releases/latest. Preview status is described in release notes. Repository visibility changed to public at the user's explicit request.


### Desktop Classic chrome and magnetic windows (2026-11-08)

Replaced the flat gray desktop surfaces with darker purple gunmetal, inset displays, double bevels and gold title rails. Revised compact Player transport/display and centered DJ platters. Title dropdowns remain the module navigator. Native Lab arrangement now has the shared title/dropdown/window controls too.

Docking geometry tests cover edge/corner snapping, distant windows, connected groups and snapping from any group member. Synthetic title-drag input verifies group selection and Shift detachment. The actual Windows native-viewport smoke test observed Player/Library edge snap, moving both windows by 30 x 20 logical points, and preserved attachment; all three assertions passed. UI captures were inspected locally. Workspace tests and Clippy with warnings denied passed. Windows installation updated after verification. External MP3 Lab remains a separate process and is not magnetically grouped; Wayland lacks global positioning, so native compositor drag is the fallback. Android 0.1.9 is unchanged.


### Desktop portable preparation engine (2026-11-08)

Added the independent AMP heuristic analyzer, Library preparation review/editor, background batch action, headless amp-prepare tool and DJ manual/grid cue buttons. Version-1 adjacent JSON now stores key/harmonic label, BPM, grid offset, cue collections, protected manual overrides, method/scores, duration and a SHA-256 source fingerprint. Legacy sidecars and extension fields round-trip. Atomic metadata replacement, invalid cue/schema validation, source-change rejection and manual-cue preservation have tests.

Workspace tests and Clippy with warnings denied passed. The opt-in real FFmpeg decoder test passed. Generated audio regression measured the 120 BPM/C-major fixture at 120.3 BPM/8B, found the 1s/9s silence boundaries, preserved a manual 2s/8s range plus named cue/extension data during re-analysis, and re-read the same fingerprint after relocating the folder. Original audio hash remained unchanged. This synthetic coverage does not validate real-song accuracy, variable tempo, musical downbeats/phrase/drop recognition or parity with Mixed In Key. The Windows installation includes the new engine and CLI. Android importer/override UI and Remote metadata transfer are still pending; the phone APK was not changed or installed in this milestone.


### Shared desktop DJ mixer and Auto Crossfader (2026-11-08)

Replaced independent desktop FFplay decks with one CPAL device callback fed by bounded background FFmpeg decoders. Added the central AUTO CROSSFADER button/manual slider, duration, channel gain/meters, pitch, transport cue, play/pause, prepared-BPM matching, captured loops, three-band EQ, low-pass FX and measured envelopes. Automatic equal-power fades start a buffered incoming deck and pause the outgoing deck on completion.

Deterministic mixer tests passed for continuity/no silent transition frames, incoming-buffer wait, pause, pitch, loop position and manual fader cancellation. The opt-in real Windows device test passed in both directions using two FFmpeg fixtures, muted device output, stable pause position and zero buffer underruns. Workspace tests, Clippy with warnings denied and release build passed. Actual DJ layout captured locally in test-output/desktop-dj-crossfader.png. This establishes a working initial mixer, not proof of beat-phase sync, key lock, perfect NAS continuity or mastering-quality DSP. The ongoing library preparation process was left running. Android and the published 0.1.9 APK were unchanged.


## Scrolling DJ waveforms (2026-11-08)

Replaced live top envelopes with source-decoded 20 ms min/max waveforms, fixed playheads, saved cue overlays, prepared beat grid and shared zoom. Absolute playback position includes the decoder cue-in offset. Tests cover cue movement past the playhead and a real FFmpeg fixture with silence followed by signed audio; cancellation is also exercised. Workspace tests and strict Clippy pass. Native screenshot: `test-output/desktop-dj-scrolling-waveforms.png` (muted fixture loaded into both decks). Analysis of the user's NAS library continues independently.


## Attached Library, explicit merge and Smart Lists (2026-11-08)

Workspace tests and strict Clippy pass with energy/key/tempo ordering, unknown/repeated input handling, similar-track filtering, sidecar extension validation/round-trip and title rescan tests. A UI test sends pointer press/release events to the real magnet and detach buttons and checks ownership changes. Layout tests reject cycles and preserve descendants on detach. Native Windows snapping/group movement smoke reports all three checks true after adapting to the compact transport height. Captured the actual combined Player/Library in Classic, Modern Light and Modern Dark with real fixture audio; reviewed Classic and Light for layout and contrast. CI now captures the combined view in all themes as well.

The loaded fixture is muted during screenshots. No generated artwork or invented songs/measurements are placed in the production library. Existing NAS batch process PID 5980 was left running; a Player-only install does not replace it. See LIBRARY.md for explicit limits: stacked/session-only composition, external MP3 Lab excluded from merging, portable tag edits rather than embedded-tag rewrite, energy as an RMS proxy, speed without key lock, heuristic recommendations rather than guaranteed transitions.


## Nine-pad sampler (2026-11-08)

Workspace tests and strict Clippy pass. Deterministic mixer tests trigger all nine voices, verify summed output, crossfader independence, retrigger, individual/all stop, one-shot completion, master mute and acknowledged buffer reclamation. Saved bank paths/levels round-trip. A real FFmpeg fixture decodes to device-rate stereo; a missing file is rejected. A muted native CPAL test loads and triggers all nine pads and verifies individual/all stop through the running device callback. CI includes sampler screenshots in all three themes. Installation updates only Player and leaves the running NAS analyzer unchanged.


## Revised show-control and protected-preparation milestone (2026-11-08)

Workspace tests, strict Clippy and release builds cover the new control gateway, saved show rules, metadata store, review decisions and per-sample fades. Native muted tests exercise Call Song + Cue, idle-deck Load Only while the outgoing song continues, Auto Fade, Manual DJ Lock, Duck/Restore and a real 9 → 7 → 5 → 3 sequence through completed transitions. The runtime test compares preparation files byte-for-byte before/after. Source audio is copied from a generated fixture and no personal library is used in these tests.

The protection regression preserves reviewed generated cues, grid, BPM, analysis and loop extensions while producing a separate candidate. Field review rejects the grid independently of accepting another field. Explicit hot-cue numbers and unknown cue fields survive. A canonical `.ampverse` record wins over a legacy adjacent sidecar without modifying that legacy file. Guarded saves hold a cross-process file lock and compare the exact revision snapshot; a stale background write is refused after newer approved metadata appears. Both the Library editor and batch worker read a fresh single snapshot rather than trusting the old library scan cache.

`tools/smoke_show_control.py` tests the actual desktop HTTP endpoint, catalog, password rejection, accepted commands versus observed feedback, duplicate retry without state replay, ID conflict, Manual DJ Lock and two passive WebSocket connections. It generates a silent local library and terminates only its own helper desktop process. CI runs it under Xvfb and captures Rules and recommendation views in all themes. Native hardware tests remain opt-in where no device is available. Full revised-spec acceptance status and unbuilt areas are listed in SPEC_STATUS.md.


## Attached-window visibility fix (2026-11-08)

Bound pane heights inside the shared outer scroll area so attaching DJ below Library cannot place it beyond an unbounded Library. Removed nested group scroll areas. The magnet is now a compact drawn horseshoe next to minimize, lights when attachment is available, and detaches an embedded pane. Removed the redundant Player magnet row button. Tests click the actual title-bar control for merge/detach, preserve alternate host ownership and assert finite reachable DJ bounds beneath Library. Player tests and strict lint pass; optimized Windows build and installation verified.


## DJ Library source sidebar (2026-11-08)

Added a left sidebar below the mixer: Auto DJ switch, saved Smart Playlists, Playlists and folder tree. Right pane filters by source and search and loads either deck. Saved playlists open for browsing without replacing the live queue, retaining track order and repeats. Smart previews show rule-eligible tracks; actual Auto DJ selection remains governed by the performance engine. A chosen Smart Playlist can be loaded paused or started through the Auto DJ switch. Source browsing does not write preparation metadata. Player tests and strict lint passed; CI captures the dedicated DJ Library view in each theme.


## Multicore song preparation (2026-11-08)

Library batch analysis and the headless helper now share bounded song workers. Tests prove three callbacks overlap, each song executes exactly once, stop prevents new work, and automatic/manual limits respect available CPUs and batch size. Real FFmpeg regression analyzes three generated songs with three workers, checks BPM/key, preserves approved metadata and original audio hashes, and validates relocated metadata. All 54 regular workspace tests and strict Clippy passed; all release binaries built. Native Library screenshot confirms 16 logical CPUs detected and an Auto limit of eight parallel songs. No throughput multiplier or playback-underrun guarantee is claimed; NAS and codec costs affect actual performance.


## Shared-library coordination / headless Cast foundation (2026-11-08)

The regular workspace tests and additional offline-approval/document-queue/proposal-coexistence regressions passed. Strict Clippy and optimized Windows builds passed. Core coverage includes competing claims, real timed subprocess-crash lease recovery, expired publisher rejection, mismatched-track lease rejection, same-cue conflicts, independent energy/waveform merges with per-field authors, concurrent playlist/rule edits, legacy migration, Windows-to-Linux playlist paths, malformed/missing claims, interrupted temp files, preserving a lock when an unrelated descriptor closes, local document queuing during share loss, and retaining concurrent analysis proposals without undoing an unrelated approved cue.

Two muted native performance tests passed: shared metadata/rescan retains the playing source offset/BPM until explicit reload, and show-control transitions/progression preserve preparation files. Refresh remaps queue/loaded indices without stopping playback, including duplicate playlist entries. Candidate selection uses the loaded snapshot too.

Native generated-audio smoke checks passed for two participating devices analyzing six tracks exactly once, byte-for-byte reuse, two independent server/station identities on the same Library ID, wrong-server command rejection, isolated station start/stop, persistent enabled state and resumed headless analysis. Preparation smoke also covers approved/manual fields, protected first-time proposals, avoiding repeated computation after dismissing a suggestion, and relocating metadata. Existing desktop HTTP/WebSocket and two-listener MP3/decode checks passed. Web JavaScript syntax passed Node's checker.

Rust 1.89 headless compatibility check and static ARMv7-musl server/analyzer cross-build passed. ELF inspection reports 32-bit ARM, hard-float EABI5 and no dynamic interpreter. This does not prove QNAP execution. Native Linux, container and emulated ARM checks are defined in server.yml and must have their actual CI results checked; LAG, AUHUBNAS and Sager hardware deployments remain unverified. Linux uses OFD byte-range locks instead of older-SMB-local flock semantics, and actual cross-host locking remains a required LAG gate.

A short Windows idle sample (16 logical CPUs, one generated track, station/analysis disabled, two-second sample) measured 6,934,528 bytes working set, 1,904,640 bytes private memory and no measurable process CPU increase. This is a small workstation observation, not a NAS benchmark or endurance claim. Local evidence lives in ignored test-output logs/artifacts.

No production NAS services, real music, network isolation or Android APK were changed. Remaining server/client features and deployment gates are explicit in SERVER_DEPLOYMENT.md and SHARED_LIBRARY_COORDINATION.md.


CI follow-up: headless Linux unit/native smoke checks and the static ARM32 QEMU health check passed in [Headless Cast candidates](https://github.com/gitwide-cloud/amp-verse/actions/runs/42522092082). The first GNU ARM container builds lacked cross-libc startup objects because recommended packages were intentionally disabled; the Dockerfile now explicitly installs the ARM64/ARMhf development runtime packages. Container publication remains gated on each image passing health checks. These checks do not replace LAG hardware validation.

Final candidate verification: [cf1270a headless CI](https://github.com/gitwide-cloud/amp-verse/actions/runs/42523040432) passed Linux foundation tests, static ARM32 execution under QEMU, and all three container candidates (AMD64, ARM64 and ARMv7). Each container analyzed a generated WAV, started a station, served MP3 and decoded the stream before publication. [Desktop CI](https://github.com/gitwide-cloud/amp-verse/actions/runs/42523040342) and [Android preview CI](https://github.com/gitwide-cloud/amp-verse/actions/runs/42523040330) also passed. Current optimized Windows binaries passed preparation and two-device/server smoke checks and were installed locally; installed hashes match the build. The existing player process was preserved and must be reopened to use the new coordination code. No physical NAS execution or cross-host NAS locking is claimed.

## Optional desktop analysis add-on (2026-11-08)

Workspace unit tests and strict Clippy passed after splitting the process boundary. The player and shared core compile independently without the analysis feature. Generated-audio subprocess checks passed for protocol-version rejection before writes, analysis publication, byte-identical incremental reuse, path confinement and protected manual draft analysis. The desktop client was also tested against the real optimized engine executable; missing executable tests confirm no embedded fallback and no library mutation. The Windows player and engine were built separately. Linux packaging/screenshot checks are defined in CI and must be checked before claiming Linux runtime verification for this change. Physical NAS validation remains outside this local test.

## DJ resize and Smart Playlist source selection (2026-11-08)

Workspace tests and strict Clippy passed. Native-window input regression drags the borderless corner and verifies a native SouthEast resize command. A 900x600 layout regression verifies the DJ container remains within viewport height; existing attached Library/DJ reachability tests still pass. Native windows explicitly allow resizing, clamp to monitor size, and expose edge/corner hit regions plus a visible grip. DJ's default native height is 760 points; oversized content scrolls in both directions.

Smart Playlist source modes now expose all music, checked folders (including subfolders), or selected songs/imported playlist membership. Source preview counts use the same eligibility predicate as Auto DJ. Regression tests cover empty selection, nested folder inclusion, exclusion of unrelated songs, imported membership, and persisted empty-source protection. Importing a playlist copies its membership into the draft; it is not a live reference to subsequent playlist edits. Existing revision/conflict checks remain in effect. The editor stays in AMP Library; DJ provides a shortcut to the selected rule editor.

## Android AMP Cast client (0.1.10)

APK and instrumentation APK build; Android lint passes with zero errors. Native CastClient/CastPanel now replace the placeholder screen. Added an opt-in device integration test (`CastDeviceTest`, instrumentation argument `castUrl`) for an isolated generated-audio server: rejected bad password, selected-server status/queue, start, actual phone MP3 playback, independent phone stop and station stop. Address validation has a separate instrumentation test. These device tests are compiled, not yet verified on hardware: the Pixel 7 (installed 0.1.8) disappeared from ADB during the attempted update. No successful installation or device runtime pass is claimed at this point. The APK retains application ID and build-machine preview signing; installation never uninstalls app data on a signature error.

### Pixel 7 Cast installation and runtime follow-up

After USB debugging was toggled, ADB returned to `device`. Installed 0.1.10 (versionCode 11) over 0.1.8 with app data retained. `CastDeviceTest` passed on the physical Pixel 7: **OK (2 tests)**, 8.536 seconds. The test used an isolated password-protected Windows Cast server, generated 12-second sine audio and a temporary USB reverse connection. It verified bad-password rejection, real status/queue, native Start/Stop controls, actual MP3 playback through the phone's media service, phone-stop/server independence and rejected ambiguous server URLs. Phone playback state and saved server preference were restored by the test. The temporary reverse and fixture server were removed. The updated app remains installed with AMP Cast open; screenshot `test-output/pixel-cast-0110.png` was visually inspected. This verifies the client over USB forwarding, not home-LAN discovery, firewall configuration, offline downloads or Google Cast receivers.

## Search and compact DJ update (0.1.11)

Android 0.1.11 (versionCode 12) was installed over the existing Pixel 7 app without clearing data. Physical-device instrumentation passed: **OK (4 tests)**, 16.109 seconds. Checks cover protected Cast connection, real MP3 listener playback and independent station/phone stop, UDP discovery parsing/deduplication, and filtered local-song playback retaining the original queue index. The search assertion waits for the media-session command to propagate. Tests restore the prior library and playback state. Discovery was verified using a phone loopback responder; Cast streaming used an isolated server over temporary USB forwarding. Physical home-LAN discovery and firewall traversal remain unverified.

Rust workspace tests and strict Clippy passed. The 900x600 DJ regression checks actual deck transport and auto-fade bounds; the optimized native screenshot was visually inspected, including master volume. Mixer, Music library and Sampler tabs prevent tools from pushing transport controls below the window. Optimized Windows player and Cast builds were installed locally. The running older player was preserved and needs reopening.

The real-server discovery smoke passed: response identity and advertised HTTP port match status, malformed requests receive no response, and discovery does not start a station or analysis. Android build and lint passed. CI execution after this commit is separate from these local results.

## Dynamic desktop window fitting

Desktop magnetic groups now fit into their available viewport through a uniform scene transform. Each panel receives a finite share of the group height, and smaller windows scale controls and pointer hit regions together instead of growing an outer horizontal/vertical canvas. Larger windows expand the available layout. Inner music lists and editors retain their own scrolling.

Player tests passed (34 passed, 7 hardware-dependent tests ignored), with strict Clippy passing. A repeated-resize regression checks both deck transport rows and the auto-crossfade button in actual transformed screen coordinates at 1200x900, 740x480, 540x360, then 1000x760. Existing magnetic merge/detach and attached Library/DJ tests pass. Very small or heavily merged windows necessarily make controls smaller; detach panels for more working space.

## Compact and taskbar mini players

Added a 520x210 compact view and 620x100 floating taskbar mini view, both scaled with their window. They reuse the active desktop playback engine and queue. Full restores the existing magnetic layout; compact views temporarily omit attached panels without closing or detaching them. Window-level pinning is optional and Full restores normal window level. These are app windows, not a native taskbar toolbar or taskbar thumbnail controls.

Player unit tests and strict Clippy passed. The view-switch regression verifies loaded decks, play state, queue, library visibility and magnetic parent relationships survive Compact, Mini and Full transitions. Native preview screenshots are generated for both views. No new hardware audio or Linux window-manager behavior is claimed from these layout checks.

## Reference-based desktop organization and vectors

Player tests: **36 passed, 7 hardware-dependent tests ignored**. Strict Clippy passed. Navigation regression clicks the real vector Sources control under Modern Dark and confirms page change without altering loaded decks, play state or queue membership. Existing resize, magnetic merge/detach, compact mode, playlist ordering and protected-playback snapshot regressions pass.

Built optimized Windows previews for combined Player/Library, Library, Playlists, Analyze Music and DJ. Visual review corrected empty-list card sizing, modern text spacing and the shared theme font scope so larger Library text does not push DJ controls outside its window. CI screenshot coverage now includes Playlists, Analyze, Devices, Compact and Mini in all three themes. Local UI checks do not establish physical LAN discovery, cross-device history, NAS service inventory or Linux window-manager verification.
