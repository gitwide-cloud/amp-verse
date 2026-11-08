# MP3 Lab Android editor integration

Source repository: https://github.com/gitwide-cloud/mp3-lab
Inspected source HEAD: `d4a0f4a7c3fc8f990ca5b2f774ea63d4cabe4755`.
APK reference: `mp3-lab/dist/v0.2.2-preview.27/mp3-lab.apk` (read-only local reference).

The four Android Java editor files were copied into `android/app/src/main/java/cloud/gitwide/ampverse/lab/` with package/resource integration, shared theme adaptation, AMP Lab branding, module navigation and an isolated `amp-lab-editing` session folder. The editor algorithms/command builder/waveform/session implementation come from MP3 Lab, not a newly written second editor. Source repository and APK were not modified. Keep this provenance and MIT notice on future updates.

Runtime remains the same pinned `dev.ffmpegkit-maintained:ffmpeg-kit-audio:8.1.9` dependency, dynamically linked. See THIRD_PARTY_NOTICES.md for corresponding-source/rebuild terms. APK notice assets are carried over from the original editor. Any release that distributes this runtime must also supply the corresponding upstream source bundle as described there.

Corresponding-source bundle is preserved unchanged alongside this file: `android-third-party-sources.zip`, SHA-256 `7387f541644d5f82494bfe97fab1d16c6fd534f62858b069da404573b577040f`. Original Java source hashes are recorded in SOURCE-SNAPSHOT.sha256.
