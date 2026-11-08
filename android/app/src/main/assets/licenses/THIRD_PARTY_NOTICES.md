# MP3 Lab third-party software

Original MP3 Lab code is MIT licensed; see LICENSE. This does not relicense its dependencies.

Desktop uses eframe/egui (MIT OR Apache-2.0), rfd (MIT), and their dependencies.
Each desktop ZIP includes dependency versions, license expressions and the license/notice files
distributed by the resolved Rust crates in `licenses/`. Cargo.lock records exact versions.
FFmpeg, ffprobe and ffplay are separate programs installed by the user, not bundled in desktop ZIPs.

Android dynamically links the unmodified `dev.ffmpegkit-maintained:ffmpeg-kit-audio:8.1.9`
libraries under LGPL-3.0. FFmpeg copyright belongs to its contributors; FFmpegKit was originally
created by Taner Sener and is maintained by the ffmpegkit-maintained project.
The audio build includes LAME, libilbc, Ogg/Vorbis, opencore-amr, Opus, Shine, SoXR,
Speex, TwoLAME and vo-amrwbenc, each under its own license. smart-exception-java 0.2.1
and smart-exception-common 0.2.1 (BSD-3-Clause) are used by the wrapper. The NDK libc++ runtime
has LLVM runtime license terms, also included. License texts and copyright notices are included in APK assets/licenses.
The app's Licenses button opens these notices.

Corresponding upstream source and build scripts are preserved in the release asset
`android-third-party-sources.zip`. The wrapper is pinned to commit
469e560b89a9ba4cab357d2c306c676b0c611745 (v8.1.9-lts-android); the archive includes
external library source archives selected by that commit's source.sh and a checksum manifest.
Upstream: https://github.com/ffmpegkit-maintained/ffmpeg/tree/469e560b89a9ba4cab357d2c306c676b0c611745
Build recipe: .github/workflows/build-81-free-audio.yml inside the upstream source archive.

Users may modify and replace the LGPL libraries and reverse engineer the combined application
to debug those modifications. Rebuild the AAR using the upstream audio recipe and NDK r27c,
replace the Gradle implementation dependency with `implementation files('libs/modified.aar')`,
and build this MIT-licensed app with Gradle 8.9 / JDK 17. Sign the resulting APK with your own
key; uninstall the preview first if its signing key differs (back up exported files first).
No production signing key or proprietary component is required to rebuild MP3 Lab.

The retro player styling is independently implemented. MP3 Lab is not affiliated with Winamp.
