# Analysis engine research and revision 3

Mixed In Key publicly describes key/Camelot, BPM, energy and up to eight editable cues at musical boundaries. Its implementation is proprietary; AMP does not contain or claim its algorithms or accuracy.
https://mixedinkey.com/integration/introduction/

Open-source alternatives reviewed:
- CueIt (MIT, alpha): Rust tempo/key, beat/downbeat grids and energy-driven intro/drop/breakdown/outro suggestions. Its own README notes cue heuristics are still being tuned. https://github.com/CurePortal/Cueit
- libKeyFinder (GPL-3.0): dedicated musical-key detection. No source linked or copied into AMP. https://github.com/mixxxdj/libkeyfinder
- librosa: onset correlation and tempo-consistent beat selection, with explicit unknown/silent handling. https://librosa.org/doc/0.11.0/generated/librosa.beat.beat_track.html
- Zehren et al., Automatic Detection of Cue Points for DJ Mixing: feature/novelty-driven switch-point research. https://arxiv.org/abs/2007.08411

AMP changes are independently implemented; no upstream source or models are vendored. Revision 3 searches fractional tempo and whole-track onset phase, caps transient weights, then fits the supported onset times to reduce grid drift. It improves pitch-class extraction with interpolated log-spectrum peaks and adds sustained energy-fall breakdown candidates alongside existing high-energy beginnings/drops. Energy candidates snap to the nearest beat when a reliable grid exists. Audio boundaries remain exact, and existing manual/approved fields remain protected with reviewable proposals.

Limits: constant tempo estimate, 60–180 BPM; half/double-tempo ambiguity remains possible. Beat phase is not a verified bar/downbeat. Breakdown labels are relative-energy suggestions, not semantic verse/chorus recognition. Existing energy remains an uncalibrated proxy. Synthetic regression tests are not a real-music accuracy benchmark. No promise of Mixed In Key parity.

The optional engine ID advances to amp-prep-heuristic-3. Existing revision-aware coordination permits an explicitly requested analysis to refresh older results, while newer results and manual changes remain protected. Installation alone does not reanalyze the library. Loaded playback snapshots remain unchanged; prepare/reload a track to use updated analysis. No new Python, neural-model or GPL dependency is added to the base player.
