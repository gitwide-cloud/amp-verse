# Third-party notices

AMP Verse original code is MIT licensed. It is not affiliated with Winamp.

Rust dependencies are resolved in `Cargo.lock` and retain their respective licenses. Key direct dependencies include CPAL (Apache-2.0), rtrb (MIT OR Apache-2.0), eframe/egui (MIT OR Apache-2.0), rfd (MIT), serde/serde_json (MIT OR Apache-2.0), RustCrypto sha2 (MIT OR Apache-2.0), Tokio (MIT), Axum (MIT), bytes (MIT), async-stream (MIT), ureq (MIT OR Apache-2.0), and test-only tempfile/tower (MIT). Consult each dependency's source license for the complete terms, including transitive dependencies and embedded fonts. Before public binary distribution, generate and review a complete dependency license bundle.

Desktop FFmpeg and FFplay remain external user-installed tools; no desktop codec runtime is bundled. Android AMP Lab now includes the same dynamically linked FFmpegKit audio runtime as the MP3 Lab editor. Their licenses depend on the build configuration. Any future bundled runtime must include matching license notices and required source/rebuild materials.

Android AMP Lab reuses the four MP3 Lab Android editor source files under MIT, retaining `Copyright (c) 2026 MP3 Lab contributors`. See `third-party/mp3-lab/` for the original MIT license, provenance/source hashes, runtime notices and the preserved corresponding-source bundle. The separate MP3 Lab repository is unchanged.

Android preview additionally uses AndroidX Media3 (Apache-2.0), AndroidX Test (Apache-2.0) and their transitive dependencies, including Guava (Apache-2.0). The supplied user icon is copied unchanged. AMP Lab includes the pinned `dev.ffmpegkit-maintained:ffmpeg-kit-audio:8.1.9` dynamically linked runtime (LGPL-3.0); its bundled dependencies retain their own notices in APK assets/licenses. Corresponding source and rebuild/replacement instructions accompany Android artifacts in `third-party/mp3-lab/`.
