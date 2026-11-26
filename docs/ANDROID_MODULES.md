# Android editions and signed modules — 0.3.0 preview

Lite and Full use `cloud.gitwide.ampverse`, one signing identity and one monotonically increasing version code (34 for this preview). They replace one another; they cannot be installed side by side. Installing with Android's normal update path retains the base application's private data. Never uninstall the base application as an upgrade workaround.

Full embeds DJ, Lab, Cast, Podcasts and Remote. Lite includes Player, local Library, Add-ons, and the small service-owned playback/DJ runtime needed by signed companions. Lite excludes Lab's FFmpeg/Python/yt-dlp/QuickJS payload and the Google Cast SDK. Neither edition contains desktop analysis models or stem-separation models.

## Playback and data boundary

The base `PlaybackService` remains the only phone music/DJ playback owner. Companions bind its Media3 session and use `content://cloud.gitwide.ampverse.modules`, contract `v1`. The provider requires a signature-level permission and independently checks the caller UID's signature. Media3 connections also check trust/signature. No dynamically downloaded DEX, native plug-in executable, shared UID, or access to another app's private paths is used.

The provider returns paged song summaries, playlist summaries and bounded engine-state snapshots. Catalog reads and playlist preparation run off the UI/audio thread; only engine mutations run on its main thread. Offline playlist downloads run in the base application through the existing incremental `RadioOffline` publisher, preserving prepared metadata and local cue overrides. Module settings and podcast subscriptions/download records also stay in the base app. Offline edits are not automatically uploaded to a NAS.

Lab is an editor with its own preview transport. Entering it pauses the music/DJ transport. Original URL downloads and editing copies are separate. A Lab companion's private editing files are not yet automatically migrated from an embedded Full Lab session; switching editions retains those original base-app files.

## Packages

| Module | Companion application ID | Current implementation |
| --- | --- | --- |
| DJ | `cloud.gitwide.ampverse.dj` | Core-owned A/B and Cue Deck controls, song loading, saved playlist Auto Mix, crossfader, fade stepper, hot cues and saved sampler playback |
| Lab | `cloud.gitwide.ampverse.lab` | Shared native editor, source waveform/trim/fades/gain/normalize, MP3 preview/export, Lab-only YouTube URL import |
| Cast | `cloud.gitwide.ampverse.cast` | LAN discovery, station controls/listening, host live-broadcast request, Google Cast picker and core-owned offline playlist sync |
| Podcasts | `cloud.gitwide.ampverse.podcasts` | Shared native directory/RSS browsing, episode preview, subscriptions and core-owned offline episode downloads |
| Remote | `cloud.gitwide.ampverse.remote` | Shared native desktop discovery/control/browsing UI, desktop A1–A3/B1–B3 selector and existing remote operations |

Full uses the existing richer native DJ/Cast surfaces and shared engine/protocols. The companion DJ surface currently exposes fewer controls; do not describe it as complete desktop parity.

## Add-ons and GitHub

Add-ons has Discover, Installed and Updates views. APK downloads require a compatible API/core/SDK/ABI, matching package ID, expected byte count, SHA-256 and the same APK signer as the installed base. The package manager supplies final installation approval. Rejected/cancelled installs retain installed applications and media. A denied unknown-source permission does not cause an endless settings loop. Disabling a module retains its data; uninstalling uses Android's own confirmation.

`tools/package_android_modules.py` reads actual Gradle outputs, verifies APK signatures and embedded package/version metadata, verifies companion interface metadata, checks that Lite contains no editor/downloader native payload, and emits versioned APK filenames, checksums, `amp-modules.json` and `android-packages.json`. Debug previews may discover prerelease catalogs; production builds only accept non-prerelease release catalogs. A local build or generated catalog is not evidence of a published release.

Production release jobs require the existing `AMP_ANDROID_*` signing secrets and fail without them. Local previews use the existing phone-compatible development signing identity; do not substitute an unrelated CI debug key for an installed preview.

## Lab URL import

Paste or inspect one HTTPS YouTube video URL, then Download MP3. Playlist parameters on a video link are stripped; playlist-only/channel/untrusted URLs are rejected. MP3 at 256 kbps is automatic; conversion does not improve source quality. Downloads use pinned `youtubedl-android` 0.18.1 for its Android Python/yt-dlp/QuickJS runtime and the existing FFmpegKit audio library for conversion. No desktop executables, cookies/sign-in bypass, DRM handling, or runtime executable downloads are added.

The downloader launches structured process arguments in its own native process group; Cancel kills that group and cancels its own FFmpeg session. Incomplete files belong to an isolated pending directory. Validated MP3s are atomically moved to unique persistent names under the app-private `AMP Lab/downloads` folder. Downloaded audio can be reopened from Lab. It enters the existing source-copy/waveform opening pipeline only after conversion and duration validation. Existing edited clips require an explicit replacement choice.

The downloader wrapper is GPL-3.0. Full/Lab distribution must include its license and corresponding-source information, alongside existing FFmpeg/Python/component notices. See the Lab assets' `licenses/youtubedl-android` directory. Other editions/modules do not link this downloader.

## Verified and remaining work

Local checks have built both editions and all five signed companions, run unit URL validation, and passed Android lint. Pixel tests cover contract/signature validation, rejection of a foreign caller, the real packaged Python/QuickJS/yt-dlp entry point, cancellation against a stalled local server and playable local 256 kbps conversion with an unchanged original. Full companion-screen checks include a real engine fade-time change and restoration. Edition-switch and latest-build UI checks must be recorded separately from those results.

Live YouTube MP3 downloading, physical Google Cast receiver playback, long screen-off/endurance operation and the full device/layout matrix have not been established by these checks. Do not call local fixtures live-download verification.

Lab now includes an offline multitrack timeline (16 clips, eight lanes), non-destructive trim/gain/fades, waveform thumbnails, project save/relink, continuous MP3 export, and five-second clip export to revision-protected DJ sampler slots. Timeline rendering and sampler persistence/conflict protection have passed Pixel tests. Entering Lab leaves music playing; auditioning requests temporary Android audio focus.

Remaining from the broader mobile roadmap include mobile prepared-stem stream playback, richer companion DJ controls/waveforms, podcast resume/speed/sleep/retention and desktop podcast synchronization. These are not enabled by a module manifest alone.
