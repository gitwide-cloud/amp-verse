# Third-party notices

AMP Verse original code is MIT licensed. It is not affiliated with Winamp.

Desktop MIDI input uses midir 0.11.1 (MIT, Patrick Reisert and RtMidi contributors), and mapping XML parsing uses roxmltree 0.21.2 (MIT OR Apache-2.0). Their license texts are in licenses/midir and licenses/roxmltree. Mixxx's mapping collection and JavaScript runtime are not bundled; the independently written importer implements a limited subset of its documented XML input format.

Desktop Google Cast control uses rust_cast 0.21.0 (MIT) and mdns-sd 0.21.7 (Apache-2.0 OR MIT), with Rustls and their Cargo.lock dependencies. Android Google Cast uses Google Play services Cast Framework 22.3.0 under Google's SDK terms, and AndroidX AppCompat/MediaRouter under Apache-2.0. No Google SDK code is copied into the Rust implementation. See the upstream packages and Google Cast documentation for their respective license terms.

Rust dependencies are resolved in `Cargo.lock` and retain their respective licenses. Key direct dependencies include CPAL (Apache-2.0), rtrb (MIT OR Apache-2.0), eframe/egui (MIT OR Apache-2.0), rfd (MIT), serde/serde_json (MIT OR Apache-2.0), RustCrypto sha2 (MIT OR Apache-2.0), UUID/getrandom (MIT OR Apache-2.0), Tokio (MIT), Axum (MIT), tokio-tungstenite/tungstenite (MIT), bytes (MIT), async-stream (MIT), ureq (MIT OR Apache-2.0), and test-only tempfile/tower (MIT). Consult each dependency's source license for the complete terms, including transitive dependencies and embedded fonts. Before public binary distribution, generate and review a complete dependency license bundle.

Desktop FFmpeg and FFplay remain external user-installed tools; no desktop codec runtime is bundled. Android AMP Lab now includes the same dynamically linked FFmpegKit audio runtime as the MP3 Lab editor. Their licenses depend on the build configuration. Any future bundled runtime must include matching license notices and required source/rebuild materials.

Android AMP Lab reuses the four MP3 Lab Android editor source files under MIT, retaining `Copyright (c) 2026 MP3 Lab contributors`. See `third-party/mp3-lab/` for the original MIT license, provenance/source hashes, runtime notices and the preserved corresponding-source bundle. The separate MP3 Lab repository is unchanged.

Android preview additionally uses AndroidX Media3 (Apache-2.0), AndroidX Test (Apache-2.0) and their transitive dependencies, including Guava (Apache-2.0). The supplied user icon is copied unchanged. AMP Lab includes the pinned `dev.ffmpegkit-maintained:ffmpeg-kit-audio:8.1.9` dynamically linked runtime (LGPL-3.0); its bundled dependencies retain their own notices in APK assets/licenses. Corresponding source and rebuild/replacement instructions accompany Android artifacts in `third-party/mp3-lab/`.

Desktop AMP Lab also adapts MP3 Lab's MIT-licensed desktop editor foundation (trim/fades, processing chain, preview lifecycle and waveform decoder). Copyright (c) 2026 MP3 Lab contributors. The original MIT license and integration provenance ship in `licenses/mp3-lab/` in desktop distributions and remain in `third-party/mp3-lab/` in source. The original MP3 Lab repository is not modified.


The optional desktop stem add-on installs Demucs 4.0.1 (MIT, Meta Platforms, Inc. and affiliates; https://github.com/facebookresearch/demucs), its official htdemucs pretrained checkpoint, CPU PyTorch/Torchaudio 2.6.0 (BSD-style upstream licenses), NumPy and their dependencies into a separate user-installed Python environment. Their package license files remain in that environment. These packages and model weights are not linked into or redistributed inside the base player executable. The worker adapter in this repository is AMP Verse code. Consult the upstream model/data documentation before redistributing weights or separated datasets.
The local catalog and stem archives use rusqlite 0.37 (MIT) and bundled SQLite (public domain), with their upstream license notices retained by the dependency packages. They are embedded file databases and require no separate database server. https://github.com/rusqlite/rusqlite and https://sqlite.org/copyright.html
Android Remote uses OkHttp 4.12.0 (Apache-2.0, Square and contributors) for authenticated WebSocket state updates. See https://github.com/square/okhttp/tree/parent-4.12.0 and its Apache license. The Cast server also uses tempfile (MIT OR Apache-2.0) at runtime for bounded device-local decoded-audio caches; no source audio or shared preparation documents are modified by those caches.

## Signalsmith Stretch (Key Lock)

The optional pitch-preserving playback backend uses Signalsmith Stretch and Signalsmith Linear (MIT), through the pinned signalsmith-stretch-rs 0.1.3 wrapper (MIT). Sources, provenance and original licenses are in `apps/amp-player/vendor/signalsmith` and are included in desktop distributions. Windows requires the accompanying `amp-keylock.dll`; if absent, Key Lock is unavailable and ordinary playback remains supported.

Genre-tag reading uses Lofty 0.25.5 (MIT OR Apache-2.0), https://github.com/Serial-ATA/lofty-rs. Its licenses are retained in licenses/lofty. Genre reading disables cover-art and audio-property parsing and does not modify source audio.

## AMP Lab YouTube import

Lab launches yt-dlp, Deno and FFmpeg as separate processes. The pinned official yt-dlp standalone (2026.10.26) includes matching yt-dlp-ejs components, Python and its own third-party dependencies; the full upstream THIRD_PARTY_LICENSES.txt, license and project source archive accompany the tools. yt-dlp original code is Unlicense; bundled executable dependencies retain their upstream terms, including copyleft components described in that inventory. Deno 2.9.8 (MIT) ships with its project source archive, MIT notice and V8/TypeScript license notices under tools/lab-youtube/licenses. Preserve those materials when redistributing the package. FFmpeg remains the separately supplied existing audio dependency; its build-specific codec licenses and source/rebuild materials remain applicable. The optional external-tools directory is not linked into or an add-on to AMP's playback engine.

Process-tree management uses process-wrap 10.0.1 (MIT OR Apache-2.0), with original license texts in licenses/process-wrap. On Windows it creates a Job Object before the process runs; on Unix it owns a process group. URL parsing uses the existing url 2.x dependency (MIT OR Apache-2.0).

## Library metadata, artwork and podcast sources

Metadata lookup uses the public MusicBrainz API with an identifying User-Agent and serialized requests spaced at least 1.1 seconds apart. The lookup uses recording/release/artist fields and, when missing locally, community genre tags; see https://musicbrainz.org/doc/About/Data_License. Results retain MusicBrainz recording and release IDs. Album covers are obtained through Cover Art Archive, https://musicbrainz.org/doc/Cover_Art_Archive/API, with the source release URL stored alongside each selected cover. Cover images retain their individual copyrights; availability through the archive does not make an image public domain.

Podcast search calls the public gPodder directory API, https://gpodder.net/search.json. AMP does not incorporate or redistribute the gPodder server implementation. Publisher RSS/Atom enclosures are fetched directly for personal playback/offline listening; feed/audio ownership remains with the publisher. No publisher content is bundled with the application.

JPEG/PNG artwork decoding uses image 0.25.10 (MIT OR Apache-2.0), https://github.com/image-rs/image, and zune-jpeg 0.5.15 / zune-core 0.5.3 (MIT OR Apache-2.0 OR Zlib), https://github.com/etemesi254/zune-image. Original notices are included under licenses/image-0.25.10, licenses/zune-jpeg-0.5.15 and licenses/zune-core-0.5.3. DLNA output implements SSDP and UPnP AVTransport using the existing HTTP/XML libraries; no MediaMonkey code or artwork is included.

Verified portable updates use zip 4.6.1 (MIT); its license is included in licenses/zip/LICENSE. SHA-256 uses the existing RustCrypto sha2 dependency. Android update installation uses Android PackageManager and FileProvider; no third-party installer is bundled.
Update ZIP compression dependencies include Zopfli (Apache-2.0) and bumpalo (MIT OR Apache-2.0); license copies are in licenses/zopfli and licenses/bumpalo.

## Optional local speech transcription

Library's Transcribe tool downloads faster-whisper 1.2.1 (MIT) from the official PyPI index into an isolated optional Python environment. It uses CTranslate2 (MIT), Hugging Face Hub (Apache-2.0), NumPy (BSD-3-Clause), and PyAV (BSD-3-Clause, with its separately licensed FFmpeg libraries). Their installed package metadata and license files remain with the optional environment. No speech runtime is linked into the base player. Python itself remains a separately installed runtime, under its PSF license. Preserve the environment's license notices if redistributing it.

The optional model is Systran/faster-whisper-base, pinned revision ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66, downloaded from Hugging Face. It is a CTranslate2 conversion of OpenAI Whisper base, under MIT. Upstream sources: https://github.com/SYSTRAN/faster-whisper, https://github.com/OpenNMT/CTranslate2, https://huggingface.co/Systran/faster-whisper-base, https://github.com/openai/whisper. Audio is processed locally, not uploaded. Existing optional Demucs stem-separation terms continue to apply when vocals are prepared for speech recognition.

## Android Full / Lab URL downloader

Full and the Lab companion include youtubedl-android 0.18.1 (GNU GPL version 3), https://github.com/yausername/youtubedl-android/tree/0.18.1 . Those combined Android binaries are distributed under GPL-3.0; original AMP source notices and other component licenses remain in force. Lite, DJ, Cast, Podcasts and Remote do not link this dependency. The matching release includes the GPL text, exact AMP source/tag, upstream downloader source and build scripts, and the existing FFmpegKit corresponding-source bundle. See android/features/lab/assets/licenses/youtubedl-android/SOURCE.txt.
