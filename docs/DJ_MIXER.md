# AMP DJ live mixer

Load a track to Deck A and another to Deck B from the embedded library. Loading prebuffers audio without playing it. Start the outgoing deck with Play. Press **AUTO CROSSFADER** above the center slider to start the incoming deck and transition across. In a manual mix, press it again to cancel, or drag the manual slider to take over. A Smart Playlist handoff keeps AutoMix running when pressed again. At fade completion the outgoing deck pauses; its position is retained for a return transition.

The Mix value sets 0.5-30 seconds. Transitions use equal-power gains on a single device sample clock. They shorten when either track has less audio remaining, using cue ranges or probed duration. A track still loading its duration, an ended incoming track or a stopped outgoing deck produces a status message. The incoming buffer must be ready before a fade starts. This removes the old independent-FFplay process timing from desktop playback, but cannot guarantee uninterrupted output during device failures or NAS starvation. Buffer underruns are counted and displayed.

## Working controls

- Play/Pause retains decoded position. Cue returns to the prepared IN point and pauses. Stop releases that deck's decoder. Manual and generated metadata cue buttons seek through a replacement decoder; they are not instantaneous sampler hotcues.
- Pitch changes playback speed and pitch together over +/-16%, using interpolation. SYNC adjusts speed to the other deck's prepared BPM, within that range. Beat-phase alignment and key-preserving time stretch are not implemented.
- Each channel has a live gain fader and measured peak meter. The top displays are scrolling track waveforms decoded from the source audio (mono min/max peaks every 20 ms, maximum six hours). Each lane has a fixed white playhead at 30% of the width, source time, prepared beat-grid lines, green IN/red OUT markers, grey generated cues and gold manual cues. Hover a marker for its exact source time; adjust the shared 4–64 second visible span. Cue jumps, pitch changes, pauses and loops use the actual mixer position. Waveforms decode in background and show loading/errors explicitly; replacing a track cancels the previous job. These are visual displays, not seek controls. Master volume is live.
- Loop captures the next four beats using prepared BPM, then repeats the captured audio. When BPM is unknown, the button explicitly uses a two-second loop. Exit resumes after the captured section. Start the loop with enough remaining audio to capture its full length. Maximum capture is 30 seconds.
- EQ opens three split-band gain controls per channel. FX opens a per-channel low-pass filter; Bypass restores unfiltered audio. Additional FX, headphone cue routing, MIDI and OSC remain future work.

## Audio architecture and verification

A CPAL output callback mixes both voices into one stream. Separate FFmpeg workers decode stereo float PCM at the device sample rate into four-second lock-free SPSC ring buffers. The callback never waits for a NAS read or decoder. GUI controls are atomic values or bounded commands; meters and scope data come back through atomics. A small channel-gain ramp suppresses gain-on clicks; the mixer reserves headroom and clamps output peaks. This is an initial DSP mixer, not a mastering limiter or professional time-stretch engine.

Deterministic PCM tests cover continuous automatic fades, waiting for incoming audio, outgoing pause on completion, pause position, pitch, loop position and manual takeover. An opt-in Windows native-device test uses two real FFmpeg decoder workers and a muted CPAL device stream to check A-to-B and B-to-A completion, pause stability and zero buffer underruns. The native test does not replace subjective listening or long-duration NAS/device-loss testing.

The desktop Player uses this same mixer; independent Cast encoding is unchanged. Android is unchanged.


## Nine-pad sampler

AMP DJ has a collapsible 3×3 sampler above the deck controls. Load a short effect onto any pad; clicking a loaded pad plays/restarts it as a one-shot. Pads can overlap, including all nine simultaneously, while both decks play. Number keys 1–9 trigger ready pads while the DJ surface is focused and a text input is not active. Each pad has Load, Stop and Clear; right-click its main button for pad volume. The bank has its own volume and Stop All button. The crossfader affects the two decks only. Player master volume controls both decks and the sampler.

File assignments and volumes are saved in `sampler.json` beside the theme settings and reloaded in the background when DJ opens. Files stay at their chosen paths; they are not copied into the library or bundled with the app. Missing/unreadable files are reported. FFmpeg decodes stereo PCM to the actual output device rate before a pad is ready. Limits are 30 seconds and 12 MiB decoded PCM per pad; longer effects are rejected rather than silently cut. Nine buffers can use up to 108 MiB plus decoding overhead. The shared output has existing headroom and final clipping protection; set sampler/pad levels appropriately when layering loud effects.

The CPAL callback mixes the nine resident voices alongside the decks, with 5 ms file-edge attack/release, no file access or sample allocation on trigger. Retrigger restarts that pad; it does not add another voice for the same pad. Acknowledged buffer retirement keeps large sample-buffer deallocation on the UI thread. A sampler does not replace the deck loops: tempo sync, sample looping, recording, slicing and MIDI triggering are not implemented.


## Hot-cue pads

Main decks and auxiliary Cue Decks show eight numbered hot-cue pads. Click an empty pad to save the current source position; click an occupied pad to seek there, preserving the current playing/paused state. Each pad displays its slot number and source time; hover for its saved name and exact position. Right-click to replace its position or clear it. Additional saved and unnumbered analysis cues remain available under More saved / analysis cues.

Explicit pad edits are saved in portable preparation metadata using revision checks. A same-slot change from another device is rejected until the track is reloaded; unrelated newer fields are preserved. Cue names, IDs and custom fields survive position replacements. Editing pads does not reload the active beatgrid, tempo or cue-in/out snapshot. These jumps still use replacement decoders: sample-instant stutter, cue quantization and key-lock are not implemented.

The side Cue Deck remains an independent audio deck with its own transport and output selection. Send A is amber; Send B is cyan. Mixed In Key results are not imported by installing that application: an explicit compatible importer is still required.


## Continuous Smart Auto Mix

Select a saved Smart Playlist, then turn Auto Mix ON. It starts a prepared song and preloads a different eligible song into the idle deck immediately. Selection is recalculated after every handoff using current energy (or the requested energy progression), BPM, harmonic key, source restrictions and repeat rules. It is not fixed playlist order. At the outgoing cue-out/end it crossfades, then refills the freed deck and continues. Changing the target energy invalidates and recalculates the next choice. A playlist with no eligible next track reports that condition rather than silently blending the same pair.

With a Smart Playlist loaded, the centre Auto crossfade button requests the next prepared playlist song and keeps continuous Auto Mix enabled if it was already on. Without a Smart Playlist it remains a two-deck crossfade. The incoming song starts at its prepared cue-in (or the preferred approved Hype cue). Prepared BPMs are speed-matched when the ratio is within +/-16%; metadata is never rewritten. Speed matching changes pitch and does not provide beat-phase alignment or key-preserving time stretch.


## Scratch-assisted handoffs and DJ layout

Scratch assist is switchable in the DJ toolbar. When prepared BPMs differ beyond the supported +/-16% matching range, a buffered outgoing track can play a brief reverse/forward scratch, then hand off with a short 250 ms fade to the next prepared track. Compatible tempos use the normal blend. This is a transition effect, not beat-phase synchronization or a guarantee that two songs sound good together. Cancelling automation or disabling the option releases the temporary scratch control. Audio and saved preparation are unchanged.

The DJ opens at up to 1440×1000 within the available monitor. Resizing still constrains the complete surface. Larger amber/cyan transport controls and platters occupy the main decks, with side Sampler A/B or independent Cue Decks and the source browser below. Each main deck's Hot cues toggle exchanges its platter view for the eight-pad bank; it does not interrupt audio. Sampler Play/Stop/Clear controls sit beneath each bank and act on the last clicked pad. All chrome, platters and controls are drawn as scalable vectors.


## Simplified DJ surface

The default mixer follows the supplied two-deck reference. Waveforms stay visible above the decks; scratch/mix-duration options remain expandable. Auto Crossfade is a switch and mixing style is a dropdown. Main decks show Loop and Hot Cues below their transport; right-click the platter for exact seek steps and pitch reset (wheel scrolling and dragging still work). Each sampler keeps its nine pads and one Play/Stop/Clear row for the last clicked pad; right-click a bottom control for Stop All. Volume has both a slider and a numeric value. Cue Deck mode remains available on either side.


When the DJ is maximized or full screen with at least 1200×850 logical pixels, sampler banks move underneath their matching main decks as nine-pad horizontal strips. Restoring the window returns the side banks. Resizing only changes the layout: sample assignments, volumes, Cue Deck mode and active audio are preserved. The full mixer and library scale together to stay within the viewport.


## Channel mixer and live headphone monitoring

The center mixer has separate amber A and cyan B channels. Gain adjusts the input level; the vertical fader adjusts the level sent to the main mix. High, Mid and Low adjust the three EQ bands. X kills a band without losing its dial setting. Double-click a dial to reset it; drag vertically, scroll, or use arrow keys to adjust it.

The Filter dial sweeps left to low-pass, stays neutral in the center, and sweeps right to high-pass. These controls affect playback only; they do not edit the music files or saved cues. EQ bands are frequency controls, not isolated vocal/instrument stems.

Open Headphones / output and select an output device. Use a different device from the main speakers for private monitoring. CUE selects a playing deck before its channel fader and the crossfader. It does not start or stop that deck. Cue / Master blends the selected cue channels with the live master mix; Phones controls the monitoring level independently. Turning headphone output off leaves the main mix running.

When the library is enlarged, the mixer can be scrolled to reach its lower controls.


## Prepared stem controls

AMP DJ now supports optional prepared vocal/instrumental separation with independent Level, Mute and Solo controls on each deck. This is separate from frequency EQ. See [STEMS.md](STEMS.md) for setup, preparation and device-local cache behavior.

## Crossfade duration

Mix time sits below the crossfader with minus/plus buttons (0.5-second steps), a directly editable seconds value, and a slider. Changes are saved automatically as a device preference and restored after restarting. The selected time applies to manual Auto Crossfade and Smart AutoMix in Hype, Steady, and Chill styles. It affects subsequent fades; an active fade finishes its current timing. The engine can shorten a fade when a track has less audio remaining, and scratch-assist handoffs keep their short safety fade.
