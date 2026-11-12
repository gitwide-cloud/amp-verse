# Auto DJ phrasing and song length

The Auto DJ switch hides automation controls while off. Turning it on starts the selected saved Smart Playlist when possible; otherwise it opens setup and explicitly asks for a playlist and Load. Setup is distinct from a running mix. Manual waveforms, jog wheels and the mixer remain usable; manual tempo/Sync All remain in Setup. Turning off cancels automation without replacing saved preparation.

Quick mixes, Balanced (default), and Full track are saved with local transition preferences. Quick targets roughly 60–90 source seconds and Balanced roughly 100/130/180 for Hype/Steady/Chill, adjusted for entry position, available song length, transition lead time, and a minimum listening window. These are implementation heuristics, not universal DJ rules. Full track keeps the prepared end. Saved manual cue-out and explicit manual mix-out markers take precedence. Very short tracks keep their full prepared span.

Earlier exits prefer approved section-end markers, then analyzed energy-section boundaries aligned to an estimated eight-bar grid when available, then counted phrase boundaries. Energy analysis does not identify a chorus or vocals semantically. Reviewed phrase/bar anchors improve timing; counted phrase phase is only an estimate. Existing incoming energy/key/tempo ranking, ready-deck preparation and transition effects remain responsible for the handoff. Crossfade seconds remain user-controlled. This policy affects automatic scheduling only; it never rewrites cues, beatgrids, stems or shared track metadata.

References consulted:
- Native Instruments, Phrase mixing: https://www.native-instruments.com/blogs/blog/phrase-mixing
- Crossfader, How To Mix Any Genre In Three Easy Steps: https://wearecrossfader.co.uk/blog/how-to-mix-any-genre-in-three-easy-steps/

The guides support structure-aware timing, cue preparation and selecting suitable transitions. They do not establish a blanket requirement to cut every song short. The implementation uses configurable pacing rather than treating that as an absolute rule.
