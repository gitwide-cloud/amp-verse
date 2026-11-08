# MP3 Lab read-only architecture review

Inspected `gitwide-cloud/mp3-lab`, local commit `d4a0f4a7c3fc8f990ca5b2f774ea63d4cabe4755`, November 8, 2026. The local checkout was clean when inspected. Source paths below refer to that repository.

- `desktop/Cargo.toml`: Rust edition 2024, eframe 0.37/OpenGL, rfd dialogs. Reused this stack/version to keep a compatible desktop foundation.
- `desktop/src/main.rs`: one `Editor` state owns source selection, clip bounds, processing settings, waveform, preview and export jobs. Worker threads return through channels; playback uses FFplay child processes with explicit kill/wait cleanup. AMP's scanner uses that asynchronous job pattern; `amp-core::Deck` uses the same lifecycle concept. AMP's implementation is new code.
- The editor's filter construction already covers trim, fades, normalization, gain and limiter, while waveform extraction decodes stereo float samples. These are good candidates for a future optional AMP Lab process handoff or a carefully extracted processing crate. They are not transplanted into Player or claimed as AMP features.
- `desktop/src/skin.rs`: gradient metal panels, black/green displays, blue accents, gold controls and a custom chrome renderer. AMP uses a smaller initial dark/green/gold UI, not a copied skin or image of a player. Detailed skin parity is future visual work.
- `.github/workflows/build.yml`: platform builds, generated-audio checks, Xvfb screenshots, packaging and gated release publication are useful patterns. AMP has its own separate workflow and artifacts, without MP3 Lab signing keys, audio-runtime assets or release mutations.
- The existing Android editor and its pinned FFmpeg dependency remain in MP3 Lab. AMP Android has different background playback and local cache requirements; build a dedicated integrated application rather than embedding a desktop plugin or assuming editor Activity lifecycle meets playback-service needs.

No source files, assets or dependencies are copied from MP3 Lab. Existing editor code is MIT and could be reused with its copyright/license notice if ported later. This milestone references the independent editor's Releases page only. Local access to the earlier chat yielded its user requests and one attached DJ reference, but none of the generated Library/Cast mockups; no exact visual-match claim is made.
