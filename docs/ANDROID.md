# AMP Verse Android preview

Package: `cloud.gitwide.ampverse`. Android 9+; compile/target SDK 35; JDK 17; Gradle 8.9; Android Gradle Plugin 8.7.3. Media3 1.8.1 is pinned for this SDK/toolchain preview. Source and app data are separate from MP3 Lab.

## Build

Set `JAVA_HOME` to JDK 17 and `ANDROID_HOME` to an Android SDK containing platform/build-tools 35.0.0 and platform-tools. From `android/` run `./gradlew :app:assembleDebug :app:assembleDebugAndroidTest :app:lintDebug` (Windows: `gradlew.bat`). The wrapper uses the official Gradle distribution. The Android CI workflow builds/lints and uploads the preview APK; device tests require a connected device and are not claimed by that build-only job.

APK: `android/app/build/outputs/apk/debug/app-debug.apk`. It is a debug preview signed with the build machine's debug key; release signing and GitHub Releases distribution are future work. Do not assume APKs from different CI machines can update a locally signed debug install.

## Use

Open **AMP Verse**, then **Library → Import local songs**. Choose one or more audio documents. Imports are copied into the app's local storage; originals are untouched. The queue persists across activity/process restarts. Removing an entry removes it from the queue; copied files are currently retained in private storage. A storage-management UI is still needed. Imported audio is retained until app data is cleared or the app is uninstalled.

The topmost **AMP VERSE** dropdown switches Player/Library and selects **Classic**, **Modern Light** or **Modern Dark**; the theme is saved locally. Player exposes real seek, pause, previous/next and stop. Queue order plays sequentially; Repeat loops it. The media service supplies standard Android media controls. Do not claim endurance, Bluetooth/device-route coverage or screen-off reliability until verified on hardware.

## Device test / installation

Use `adb devices -l` and target the Pixel 7 serial explicitly. Install the main APK with `adb -s <serial> install -r <apk>`, then the Android test APK. Run `adb -s <serial> shell am instrument -w cloud.gitwide.ampverse.test/androidx.test.runner.AndroidJUnitRunner`.

`PlaybackDeviceTest` preserves/restores the library, generates two silent local WAV fixtures, checks atomic queue persistence, actual decoder progress and track transition, then sleeps/wakes the display and checks playback progress. It restores the library and deletes only its own fixtures. The test may briefly turn the phone screen off. No user audio is required. Initial SAF picker/theme UI checks remain manual until a dedicated UI test is added.

**User instruction: leave AMP Verse installed on the Pixel after development/testing. Never uninstall the main app as cleanup.** Do not clear unrelated app data or modify other installed apps.

Still unbuilt: folder-tree import/browser, named/reordered playlists, prepared cues/beatgrids/stems, manual cue overrides, cue-aware Auto DJ, LAN discovery/password remote control and automatic updates.
