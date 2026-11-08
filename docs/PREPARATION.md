# Current portable store and approval policy

New writes use `.ampverse/tracks/song.ext.json` under the audio file's containing directory. Legacy adjacent `.amp.json` files remain readable, with the portable record taking precedence. The root `.ampverse/show.json` holds saved Smart Playlist configurations. Copy these folders with the music. Reanalysis protects approved/manual cues, grid, key, tempo, loops and analysis records; proposed supported-field changes are reviewed in Library before explicit Save. Detailed behavior: COMPANION.md and SPEC_STATUS.md.

The following sections record the original preparation milestone and legacy format:

# Portable AMP preparation

The independent AMP preparation engine estimates tempo, tonal key, audible start/end and regular 64-beat marker candidates. It is not Mixed In Key's proprietary engine. Estimates need review: there is no commercial accuracy claim, variable-tempo tracking, downbeat/phrase/drop recognition or song-corpus validation yet.

## Desktop use

Open AMP Library and choose a local or mounted NAS music folder. Select a track and choose **Prepare selected / cues...**. Analyze it, review the values, add named manual cues and choose **Save metadata beside audio**. The batch action **Analyze library and save metadata** analyzes all indexed tracks on a background worker and saves their sidecars. The headless equivalent is `amp-prepare MUSIC_FOLDER`.

DJ reads the prepared IN/OUT and exposes up to eight manual/grid cue buttons on a loaded deck, prioritizing manual cues. A cue button restarts that deck at the stored source time; this process audio adapter does not provide sample-accurate hotcue triggering. Cast uses prepared IN/OUT. Lab mix arrangements use the same preparation ranges.

## Files and interoperability

For `Albums/song.mp3`, metadata lives at `Albums/song.mp3.amp.json`. Keep both files together when copying music to a NAS or another platform. A sidecar contains UTF-8 JSON, original-audio times in seconds, schema version 1, an optional SHA-256 fingerprint of the original audio, key/harmonic notation, BPM, beat offset, generated cue candidates, manual cues and analysis method/scores. No machine-specific absolute path is stored.

Windows and Linux share this reader/writer in amp-core. Cast reads it too. Android's existing local importer does **not** consume these sidecars yet; copying the JSON preserves the data, but Android import, local override UI and Remote transfer still need integration. Cross-platform file portability must not be confused with those unfinished workflows.

`cue_in`/`cue_out` remain compatible with earlier AMP sidecars. `cues` contains generated candidates; `manual_cues` is preserved by re-analysis. `manual_override=true` protects the top-level cue range, BPM, key and grid offset. Hand-authored legacy ranges are treated as protected. Unrecognized extension fields round-trip without removal. Future unsupported schema versions are refused. Fingerprints detect changed audio. Sidecar writes use a temporary file and rename; a batch skips metadata edited during its analysis. These checks do not provide a distributed locking/merge service for simultaneous NAS editors.

Example of manually prepared metadata:

```json
{
  "schema_version": 1,
  "cue_in": 8.0,
  "cue_out": 188.0,
  "bpm": 128.0,
  "beat_offset": 8.0,
  "key": "C major",
  "harmonic_key": "8B",
  "manual_override": true,
  "stems": [],
  "cues": [],
  "manual_cues": [
    {"id": "my-mix-in", "label": "Mix in", "seconds": 8.0, "kind": "hotcue"},
    {"id": "my-drop", "label": "Drop", "seconds": 38.0, "kind": "hotcue"}
  ]
}
```

## Analysis limits and methods

FFmpeg decodes the entire source to a bounded streaming mono signal. Energy onsets and normalized autocorrelation estimate a single 60-180 BPM tempo; windowed FFT pitch-class peaks and independent major/minor templates suggest key. Silence boundaries come from measured relative RMS level. Low-scoring tempo/key results are left unknown. Scores are heuristics, not calibrated probabilities. Grid markers are regular candidates, not inferred musical phrases. Analysis is capped at six hours per track and does not alter original audio.

Background/reference reading: [music21 pitch-profile key analysis](https://music21.org/music21docs/moduleReference/moduleAnalysisDiscrete.html), [Essentia tempo/beat estimates and confidence](https://essentia.upf.edu/reference/std_RhythmExtractor2013.html), and [Mixed In Key's harmonic-key notation](https://mixedinkey.com/camelot-wheel/). AMP does not use music21 or Essentia code/models. The harmonic labels follow the familiar wheel convention (C major 8B, A minor 8A).
