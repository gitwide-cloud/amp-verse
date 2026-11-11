# Lab mix timeline

Open AMP Lab → Mix timeline. Add audio from disk or Library, or drop MP3/audio files onto a track lane. Disk and Library imports open the full source recording. Sources are referenced, never overwritten.

- Drag a clip body horizontally to move its time, or vertically to change lanes.
- Drag its left/right edge to trim; exact Start, In and Out fields are also available.
- Click the ruler to position the playhead, then split the selected clip.
- Set clip gain and fade-in/out seconds. Overlapping clips sum together through a master limiter.
- Use zoom, Fit mix and the scrollbars for long recordings. Waveform peaks are bounded and audio reads run in background, with two imports at a time.
- Preview renders up to 30 seconds from the playhead. Stop preview ends playback.
- Save a `.ampmix` project to retain source references, lanes, timings, trims, gain and fades. Earlier arrangement files still load. Export creates a combined MP3 or individual trimmed MP3s.

This is a simple audio arrangement editor, not an Adobe session importer or an automatic beat/phrase editor. Projects reference audio files; moving a project alone does not copy its source recordings. Playback preview is rendered rather than live while dragging. Starting inside a fade-in currently previews that portion at its clip gain rather than reconstructing the partial fade envelope.

Verified: synthetic pointer interaction moves a clip between lanes and trims its edge, project round-trip and legacy defaults, invalid fades, native FFmpeg export, and a real two-MP3 layered export with overlap, trims and fades. The real test checks output duration and non-silent waveform.
