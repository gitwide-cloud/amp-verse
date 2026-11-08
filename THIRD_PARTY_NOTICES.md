# Third-party notices

AMP Verse original code is MIT licensed. It is not affiliated with Winamp.

Rust dependencies are resolved in `Cargo.lock` and retain their respective licenses. Key direct dependencies include eframe/egui (MIT OR Apache-2.0), rfd (MIT), serde/serde_json (MIT OR Apache-2.0), Tokio (MIT), Axum (MIT), bytes (MIT), async-stream (MIT), and test-only tempfile/tower (MIT). Consult each dependency's source license for the complete terms, including transitive dependencies and embedded fonts. Before public binary distribution, generate and review a complete dependency license bundle.

FFmpeg and FFplay are external user-installed tools in this milestone. No FFmpeg binary, codec library or MP3 Lab runtime is bundled or downloaded automatically. Their licenses depend on the build configuration. Any future bundled runtime must include matching license notices and required source/rebuild materials.

MP3 Lab architecture was inspected read-only; no original MP3 Lab code/assets are copied. If its code is ported later, retain `Copyright (c) 2026 MP3 Lab contributors` and its MIT license. Its independent downloads have separate third-party obligations.
