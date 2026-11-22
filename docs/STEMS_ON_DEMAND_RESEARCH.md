# On-demand stems: research and first prototype

Status: experimental Rust session core only. Not connected to the DJ, desktop/server playback or Android. The withdrawn legacy separator remains disabled. No model is bundled, downloaded or executed by this prototype. No saved stem audio or file/database APIs exist in amp-stems.

## Technology decision

Use Rust for ownership, bounded PCM windows, cancellation/generation checks and playback handoff. Evaluate a separately supervised inference worker behind the Separator trait. ONNX Runtime is the preferred deployment candidate, conditional on a tested compatible model; its execution providers include CUDA and Windows DirectML, with CPU available. Provider support does not establish model compatibility or realtime performance.

Demucs is a useful comparison baseline, not an accepted realtime backend. Its code is MIT; the maintainer explicitly says the pretrained-weight license is unclear. Do not bundle weights just because code is MIT. Newer RoFormer/SCNet models available through MSST are candidates for offline quality comparison, not automatically suitable streaming engines. Code, checkpoint license, provenance, latency, VRAM and model conversion need separate verification. Open-Unmix is another MIT-code reference; its bidirectional LSTM and spectrogram processing need measured window/seam behavior, not assumptions of causal realtime operation.

References checked2026-11-22:
- https://github.com/adefossez/demucs#license
- https://github.com/ZFTurbo/Music-Source-Separation-Training
- https://github.com/sigsep/open-unmix-pytorch
- https://onnxruntime.ai/docs/execution-providers/
- https://virtualdj.com/manuals/virtualdj/interface/stems2.html
- https://mixxx.org/news/2025-05-11-mixxx-2_6_beta-released/

## Implemented core

- A fixed six-deck session, one worker request in flight; no library enumeration.
- Stereo44.1/48kHz windows capped at six seconds. This is a maximum work unit, NOT six seconds of measured latency or a latency target.
- Default64MiB aggregate accounted PCM budget. Reserve input plus four output parts before decoding/allocating; existing cached blocks count too. Account Vec capacity, reject oversized/NaN/nonfinite output and invalid dimensions.
- The budget covers owned PCM reservations/retained blocks, not model weights, tensor workspace, transport copies or VRAM. A bad backend can allocate too much before validation; process supervision/memory limits are still required. No claim of a total-process hard cap.
- Unload/seek cancels the ticket and clears ready audio. The busy state and reservation remain until terminal acknowledgment, preventing overlapping canceled models. Cross-session tickets are rejected.
- Borrowed ready blocks only cover their original frame range. Failures cannot publish old audio to a new song.
- No disk persistence, saved WAVs, audio BLOBs, cloud uploads, model downloads or fake stem effect. Tests use synthetic blocks only to exercise control/validation; they do not test separation quality.

## Next implementation and acceptance

1. Select one checkpoint with independently verified provenance and redistribution terms. Check conversion parity for ONNX, or retain a research-only PyTorch baseline.
2. Add a read-only bounded-window model adapter and supervised worker protocol. No auto-download and no silent CPU fallback during a performance.
3. Measure actual startup delay, steady-state real-time factor, model/driver RAM/VRAM, seam continuity and quality against original references on Windows/Linux. A loop over silent test blocks is not an inference benchmark.
4. Implement context/overlap joining and playback-thread handoff, including seek/cancel/deck unload, late result rejection and model timeout/crash handling. Keep original playback untouched if the worker is late.
5. Add explicit per-deck on-demand activation. No ordinary playback/library analysis trigger, batch-library button or automatic saved audio. Server offload is an explicit policy; mobile remains a reader.

Block encountered: automatic approval review rejected an attempted model-worker adapter write with only blocked-by-policy, before execution. The adapter file is absent; no alternative write/execution route was used. Research, the independent Rust session core and its tests proceeded. No real model/audio benchmark or install is claimed.

Commands: cargo test -p amp-stems --locked; cargo run -p amp-stems --example session_bench --release --locked. The example reports control-plane/validation overhead only.
