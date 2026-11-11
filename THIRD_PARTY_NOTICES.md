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
