# Auto DJ transition policy

This engine uses deterministic timing and cached preparation, not a live AI model.
The implementation is original; references inform musical rules, not copied DSP.

## Musical references

- [Native Instruments: phrase mixing](https://www.native-instruments.com/blogs/blog/phrase-mixing)
  explains aligning arrangements and choosing musical handoff points, beyond BPM.
- [DJ TechTools: tracks without DJ intros](https://djtechtools.com/2017/09/11/mixing-songs-dont-dj-intro-4-common-techniques/)
  discusses short handoffs and entering around significant phrase changes.
- [VirtualDJ stems manual](https://virtualdj.com/manuals/virtualdj/interface/stems2.html)
  describes isolating vocals and backing for transitions.
- [VirtualDJ scripting reference](https://virtualdj.com/manuals/virtualdj/appendix/vdjscriptverbs.html)
  distinguishes beat locking, quantization and scratch/slip behavior.

## Implemented rules (desktop 1.0.6)

- Match actual playback tempos, including half/double-time interpretation. All
  automatic blend/mashup grids use the same rate-aware calculation. Reject
  mismatches above 0.05%; retain beat alignment when phrase interpretation is
  ambiguous. Explicit Immediate timing remains immediate.
- Rematch the silent incoming deck immediately before handoff. Automation must
  not call the manual Sync toggle or pull a free-running pair toward an unrelated
  master BPM. Follow an already synchronized master only for matching pulse
  interpretations. Enable available pitch preservation before the incoming deck
  becomes audible; do not change the sounding deck's pitch settings.
- A drop lead-in includes the entire planned transition, not only its last fade.
  A vocal hold's two ramps and beat-counted hold must finish at the selected
  incoming landing. Automatically prepared drop/chorus cues are preferred near
  the selected energy section. Approved source boundaries/cues remain protected.
- Vocal blends introduce the incoming vocal over outgoing instruments/drums,
  then bring in incoming backing while retiring outgoing backing. Session masks
  multiply manual stem settings and never save new stem defaults.
- Automatic scratch-ins preview incoming drums for one bar using forward/reverse
  motion on the audio clock, then rewind to the exact queued incoming entry.
  They run before the matched entry grid, not as an arbitrary UI-timed outgoing
  scratch, with a final handoff of at most one beat. The ordinary saved fade
  duration is unchanged. Preview is drum-only and does not allocate in the callback. Manual
  jog scratching remains independent. Legacy manual scratch-assist is unchanged.
- Missing/uncertain timing, insufficient audio or unavailable stems falls back
  to a clean blend. Do not run stem separation inside the live audio callback.

Energy changes are not proof of a chorus. Labelled/approved chorus and drop cues
are stronger information; an unlabelled energy boundary remains an estimate.
Sample-clock scheduling cannot repair a wrong beatgrid or a live drummer's
changing tempo. No claim of universal perfect beatmatching is made.

## Verification

Rendered callback tests cover one-vocal holds, restoration of stem settings,
scratch drum isolation, exact return to entry and identical output with 1, 64,
257 and 1024-frame callback blocks. Rate-aware grid math is checked across 1000
beats. A muted native-output test exercises real FFmpeg stem decoders through
both a mashup and scratch-in. Synthetic timing checks do not establish subjective
mix quality for every song pair; variable-tempo/downbeat analysis remains limited.
Additional muted native tests verify idempotent matching against a 126-BPM
sounding deck despite an unrelated 95-BPM master, and clean fallback without
prepared stems. The full workspace suite passed (319 tests, 56 skipped), with
strict Clippy checks; the skipped platform/fixture tests are not claimed as run.

## Artist-video research and 1.0.7

Evidence reviewed:

- [Martin Garrix's Capital XTRA masterclass](https://www.capitalxtra.com/artists/martin-garrix/dj-masterclass/tutorial/)
  and its [embedded video](https://www.youtube.com/watch?v=tNgUDuRtc_s).
  The broadcaster describes a demonstration of mixer effects for crowd energy.
  This supports studying purposeful effect placement, not an exact undocumented
  knob recipe or a claim that AMP sounds like Garrix.
- [Armin's official transition lesson](https://www.masterclass.com/classes/armin-van-buuren-teaches-dance-music/chapters/using-the-decks-armin-s-techniques)
  publicly lists overlaps, tempo changes, reverb, channel layering and loops.
  [His official year-mix preparation stream](https://www.youtube.com/watch?v=_NghSHqtquE)
  is a useful follow-up reference. Its description was accessible; the full
  four-hour audiovisual session and paid lesson were not audited here.
- [Marshmello's official Ultra 2023 set](https://www.youtube.com/watch?v=deDFAmOPYkQ)
  was located, but not fully auditioned. The practical timing rule comes from
  [DJ Carlo's Marshmello technique breakdown](https://www.youtube.com/watch?v=hBbZ5iPXld8),
  whose indexed description specifies aligning sixteen-beat buildups before
  handing over the drop. This is the educator's explanation, not a statement
  from Marshmello himself.

The new build/drop planner is an original implementation of that general rule.
It is available through **Build/drop & bass swap**, not an artist-branded preset.
It requires compatible loaded tuning, rate-matched timing, an incoming drop cue
with a complete sixteen-beat lead-in, and a nearby future outgoing drop cue.
Reviewed outgoing cues take priority. The sounding deck is never sought; source
cues and the saved ordinary fade duration remain unchanged. Missing data, an
already-passed cue, a distant cue or ambiguous half-time phrasing uses the
existing normal transition. An unlabelled energy peak is not a drop cue.

Tests cover unequal original BPMs, complete buildup length, exact landing math,
rejection of missing/late/distant cues and mismatched tempos, preservation of
cue data, and an audio-clock rendered handoff. This is not a claim of listening
to every transition in the referenced sets or learning crowd response from video.
Reverb-tail automation and full-set energy-arc learning are not added by 1.0.7.
