# Sound effects in the sampler

Press **Sound effects…** in either sampler bank (including compact favorite pads). Search the list, choose sampler A or B and pad 1–9, then press Load. This uses the existing background sample decoder and saved bank assignment; it does not trigger playback. Loading replaces the chosen pad only.

The list uses analysis already present in the current library. It suggests analyzed files up to 30 seconds whose names, source folders or genre contain sample/effect hints such as SFX, FX, Sound Effects, Samples, DJ Drops, air horn, siren or jingle. Enable **Include all short clips** to see other analyzed clips up to 30 seconds. Search matches both title and folder path.

This is a naming/duration heuristic, not acoustic recognition of a sound effect. Short songs may match, and an effect with an unclear name may need the all-clips option. Files without duration analysis appear after analysis and the library refresh. Longer audio must be clipped in AMP Lab before loading. All configured sources are eligible once present in the library.

No new list file is written. The list is derived from the library's preparation data and updates as that data changes. Original audio, library membership, cues, ratings and active deck snapshots remain unchanged. Existing sampler file loading remains available for clips outside the indexed library.

Validation: duration/name boundary and missing-analysis tests; search/result selection with sampler B and pad 8 retained in a 360×320 layout; existing sampler decode, persistence and window layout tests. Automatic detection has not been evaluated against a labeled sound-effects corpus.
