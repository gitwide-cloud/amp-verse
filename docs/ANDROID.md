# AMP Verse Android preview


## Desktop library and Android Remote ownership (current direction)

Desktop AMP Verse owns the music libraries, folder roots and playlists. Android Remote connects to that desktop/server, browses its songs/playlists and streams selected content to the phone. From that remote source, the user can explicitly Save offline; that downloaded collection belongs to the radio portion. A phone-folder import is a separate local-player source, not the definition of the radio offline library.

Target flow: desktop adds library → Android Remote connects → browse songs/playlists → listen on phone → Save offline for radio. Downloading a song or playlist must copy the actual audio and portable preparation to a managed radio cache, persist membership/source identity and report completion only once playback is available without the desktop. Offline radio items must remain usable when the source is unavailable; refresh must preserve local cue overrides. Do not cache a live station stream and label it as downloaded playlist songs. No desktop playback command should be sent when the user chooses to listen on the phone.

Current limitations: the Android build supports independent phone-file imports and direct radio-stream URLs. Desktop library browsing/streaming APIs, Remote browsing and Save offline from that source are not implemented. Retain existing imported phone audio; do not silently move, delete or relabel it as radio downloads. Direct phone-folder playback remains available as a separate local-player source, as confirmed by the user.


Package: `cloud.gitwide.ampverse`. Android 9+; compile/target SDK 35; JDK 17; Gradle 8.9; Android Gradle Plugin 8.7.3. Media3 1.8.1 is pinned for this SDK/toolchain preview. Source and app data are separate from MP3 Lab.

## Build

Set `JAVA_HOME` to JDK 17 and `ANDROID_HOME` to an Android SDK containing platform/build-tools 35.0.0 and platform-tools. From `android/` run `./gradlew :app:assembleDebug :app:assembleDebugAndroidTest :app:lintDebug` (Windows: `gradlew.bat`). The wrapper uses the official Gradle distribution. The Android CI workflow builds/lints and uploads the preview APK; device tests require a connected device and are not claimed by that build-only job.

APK: `android/app/build/outputs/apk/debug/app-debug.apk`. It is a debug preview signed with the build machine's debug key; release signing and GitHub Releases distribution are future work. Do not assume APKs from different CI machines can update a locally signed debug install.

## Use

Open **AMP Verse**, then **Library → Import local songs**. Choose one or more audio documents. Imports are copied into the app's local storage; originals are untouched. The queue persists across activity/process restarts. Removing an entry removes it from the queue; copied files are currently retained in private storage. A storage-management UI is still needed. Imported audio is retained until app data is cleared or the app is uninstalled.

The topmost **AMP VERSE** dropdown lists **AMP Player, AMP Library, AMP DJ, AMP Remote, AMP Lab, AMP Cast**, plus **Settings**. A horizontally scrollable module strip repeats the six module shortcuts. Settings contains **Classic**, **Modern Light** and **Modern Dark**, Repeat queue, Android notification settings, local storage information and App Updater. Appearance uses one palette shared by all app screens. Player exposes real seek, pause, previous/next and stop. Queue order plays sequentially; Repeat loops it. The media service supplies standard Android media controls. Do not claim endurance, Bluetooth/device-route coverage or screen-off reliability until verified on hardware.

## Device test / installation

Use `adb devices -l` and target the Pixel 7 serial explicitly. Install the main APK with `adb -s <serial> install -r <apk>`, then the Android test APK. Run `adb -s <serial> shell am instrument -w cloud.gitwide.ampverse.test/androidx.test.runner.AndroidJUnitRunner`.

`PlaybackDeviceTest` preserves/restores the library, generates two silent local WAV fixtures, checks atomic queue persistence, actual decoder progress and track transition, then sleeps/wakes the display and checks playback progress. It restores the library and deletes only its own fixtures. The test may briefly turn the phone screen off. No user audio is required. Initial SAF picker/theme UI checks remain manual until a dedicated UI test is added.

**User instruction: leave AMP Verse installed on the Pixel after development/testing. Never uninstall the main app as cleanup.** Do not clear unrelated app data or modify other installed apps.

Still unbuilt: folder-tree import/browser, named/reordered playlists, prepared cues/beatgrids/stems, manual cue overrides, cue-aware Auto DJ, LAN discovery/password remote control and automatic updates.


## App updater

Version 0.1.1 adds a manual **Settings → App updater → Check for updates** request to GitHub's HTTPS latest-release endpoint. It compares stable three-part version tags, reports unavailable/private releases honestly, and offers **Open GitHub Releases / Download**. This opens the canonical repository Releases page in the browser, where sign-in can happen if required. No GitHub credentials are stored in the app. APK download and installation are user-driven; no silent update or plugin loader is included. The Android INTERNET permission is used only for this explicit update check in the current app. Local playback still works offline.


## Compact Classic shell (0.1.2)

The reference-inspired shell uses a compact metallic/beveled title dropdown and buttons, dark information panels, green elapsed-time/track displays, local icon fallback, seek/transport row and a local queue. Modern Light/Dark reuse the same views and actions with flat surfaces. No animated spectrum, fake artwork, artist metadata or fabricated queue songs are added. The imported folder/count reflects actual local saved tracks.

AMP Player and AMP Library retain playback/import/queue-removal operations. AMP DJ exposes the currently implemented local sequential queue/repeat path, with advanced mixing marked unfinished. AMP Remote, AMP Lab and AMP Cast are navigable preview pages explaining what is not yet implemented on Android. They do not pretend to connect, edit or broadcast. Settings remains the sole theme-selection location and contains the updater.


## Scan music folder (0.1.3)

Use **AMP Library → Scan music folder** (or **Player → + Music → Scan folder**). Choose a folder and approve **Use this folder** in Android's picker. All audio documents in that folder and its subfolders are scanned on a background worker and copied to private local storage for offline playback. No storage-wide permission is requested. **Select individual songs** remains available. Android may restrict selecting storage roots/Downloads; choose an allowed music folder.

Repeated imports of the same document URI are skipped while that source is already in the saved library. This is source-identity matching, not content hashing: the same file selected through a different provider/URI, or songs imported before source tracking was added, may be copied again. The current library is still an Imported collection, not a browsable representation of the source folder hierarchy. The UI initially renders up to 100 songs with a Show more action; all imported songs remain in the playback queue. Formats depend on the Android playback engine; an audio extension is not proof of codec support.


## Player tools (0.1.4)

The player now includes **EQ**, **Shuffle**, **Repeat**, **PL** and **Radio** buttons plus a live 24-band spectrum above the track card. Shuffle changes Media3 queue traversal, Repeat loops the queue, and their selections persist. **EQ** opens five adjustable bands (80 Hz, 250 Hz, 1 kHz, 4 kHz, 12 kHz), enable/bypass and Flat reset; changes are saved and applied in the service's PCM processing chain. Band gains range from -12 to +12 dB; excessive boosts can clip. The current processing path uses 16-bit PCM with float audio-sink output disabled. No microphone/recording permission is requested. Spectrum levels come from decoded post-EQ audio, decay when audio stops and are not random animation; spectrum measurement is skipped while the view is hidden.

**Radio** plays a user-entered direct HTTP/HTTPS audio-stream URL, including an AMP Cast `/listen.mp3` URL. This is internet/network radio, not an FM tuner or station directory. HTTP streams require cleartext networking support, which is enabled for these user-selected URLs. The live stream replaces the active timeline temporarily; the saved local library is retained and choosing a local library song restores the local queue. Radio download caching, a station catalog and native Cast administration remain future work.
