# AMP Verse TV — 0.3.1 Full

The existing Full Android APK now includes a dedicated TV shell. SHIELD / Google TV launches it through LEANBACK_LAUNCHER, and the normal app entry automatically routes television devices to it. Phones keep their existing interface. All Full modules remain embedded; this is one application and playback service, not a separately maintained TV audio engine.

## TV workflows

- Wide desktop-like workspace: persistent left navigation, large content area, grey/amber AMP identity, overscan margins and readable text.
- D-pad/OK/Back navigation, bright focus outlines, standard TV text input and media keys. The navigation rail and long panels scroll when the available screen is shorter.
- Player: title/artist, spectrum, seek, previous/play/pause/next. Opening another view does not seize playback ownership.
- Library: saved local music plus synced offline tracks, name/artist substring search, folder/subfolder import via Android's document picker, manual Refresh. Music stays in its selected location; saved preparation travels through the existing library facilities.
- Playlists: existing normal/Smart Playlists, with sources resolved only after selecting a list. Offline lists are available through Playlists > Offline / synced playlists. Use Remote to save desktop playlists onto the TV; desktop/NAS music is not copied merely by opening the app.
- Simple DJ: Auto Mix toggle, Next Mix and remembered crossfade duration. Reuses prepared BPM/cues and the shared lightweight DJ logic. Enabling it during Player playback carries over the current song position. No analysis or stem separation runs on the TV. Full desktop jog/scratch/mashup controls are intentionally absent from this TV workspace.
- Podcasts: existing browse/categories/search, preview/play, subscriptions and downloads panel with larger D-pad-focusable controls.
- Devices: separate Cast/station listening and desktop Remote/sync workflows. Modules are opened on demand and closed when leaving the view.
- Settings: appearance, existing updater, Add-ons and access to AMP Lab / the standard layout. Lab's advanced editor keeps its existing interface; it has not been redesigned as a couch/remote editor.

A TV without a document picker gets a recovery message; local music can be added using a compatible TV document picker or playlists can be saved through Remote. The APK currently targets arm64-v8a, Android 9+. The connected SHIELD reports arm64 support. 32-bit-only TV models are not covered by this package.

## Verification and delivery

Full and Lite debug APKs, Full instrumented APK and Full JVM tests built successfully. On the physical SHIELD: TV launcher/touch declarations, D-pad navigation, background/resume behavior, native shared tempo calculation, normal app-to-TV routing, existing silent-audio deck handoff/recreation and playlist-source tests. Captured Player and Library screens were visually inspected at the device's 1920x1080 resolution. Tests used temporary silent fixtures and restored saved library/playlist values.

The Full 0.3.1 / versionCode 35 APK was installed in place on SHIELD without uninstalling. Prior installed APK and preferences were backed up under test-output/android-tv-device. A versioned APK and SHA-256 are retained under test-output/releases/android-tv-0.3.1. This is a locally signed development build, not a newly published GitHub/Play release. Native TV testing did not include a live podcast stream, live Google Cast receiver playback or a new desktop-to-TV offline download.

Official guidance consulted: https://developer.android.com/training/tv/get-started/create and https://developer.android.com/training/tv/get-started/controllers .
