# Small player and optional analysis

Windows and Linux keep decoding, playback, folder browsing, prepared metadata reading, manual cue editing and lightweight ordering of already prepared songs available in the player. Heavy BPM/key/energy/waveform/cue computation is an optional `amp-engine` executable, not an in-process DLL. The player does not contain its DSP implementation when built independently with `cargo build -p amp-player --release`.

Install the engine from Plugin Manager using an extracted, trusted executable for the same OS. It is copied into `addons/amp-engine.exe` on Windows or `addons/amp-engine` on Linux. `AMP_ENGINE_EXE` can explicitly select a development installation. No engine is started on player installation, startup, scanning or ordinary playback. Missing-engine analysis requests show an explanation; manual editing and reading prepared files still work. Automatic release download/update verification is not implemented yet; the Releases link is the distribution entry point.

Each analysis request starts one subprocess using protocol 1 JSON on stdin/stdout. It handles one relative track and either a review draft or a batch preparation request. The existing bounded worker pool controls concurrency. Shared leases, renewal, fencing, revision checks, suggestions and protected manual metadata remain in the shared core; the executable calls those same coordinated entry points. There is no alternate uncoordinated analysis path. A missing/crashed/incompatible engine reports an error and cannot cause the player to fall back to embedded analysis.

The core `analysis` Cargo feature is disabled by default. The optional engine and headless Cast currently enable it explicitly; Cast remains a separately deployed analysis-capable service. Workspace builds unify features, so distribution builds rebuild the player by itself after building all components. Engine artifacts are packaged separately from the base player.

This is the first executable add-on boundary. DJ and Library interfaces are still included in the desktop binary; Lab remains an external editor and Cast a companion service. Their full conversion to separately installable modules is not complete. Lightweight Smart Playlist ordering over existing metadata remains in the shared playback core; automatic audio analysis is optional. Android retains its integrated app and does not load desktop add-ons.

QNAP app/deployment work is cancelled. NAS storage remains supported subject to the documented cross-host coordination acceptance test; no analysis workers are enabled on a NAS as part of this change.

The Windows installer now installs only the base player by default. `-WithAnalysis` explicitly installs the engine under `addons`; `-WithCast` adds the existing Cast/analyzer companion executables. Existing installed companions are retained during upgrade.
