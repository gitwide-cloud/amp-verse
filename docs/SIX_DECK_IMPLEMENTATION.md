# Six-deck implementation and acceptance record

Development branch: `feature/six-deck-controls`. This is a preview implementation, not a production release announcement. The installed desktop production build is preserved while acceptance checks continue. The separate MP3 Lab repository is unchanged.

## Implemented

- Six simultaneous independent decoder/timeline voices, A1/A2/A3 and B1/B2/B3, in one native output stream. Two LIVE jog interfaces select focus independently of the assigned crossfade pair.
- Auto DJ, crossfades, section changes and transition effects operate on the assigned pair. Independent background gain, fades, loops, transport and private headphone cue remain separate. Reassignment is rejected unless both old/new pairs are paused and Auto DJ is stopped.
- A sample-clock master tempo, continuous phase when tempo changes, quantized launch, synchronized playback, and quantized loop capture for synchronized decks. Unprepared BPM is rejected for sync/beat loops.
- Signalsmith Stretch pitch preservation behind per-deck Key Lock. Prepared stems share one timeline and one pitch processor per deck. Vinyl scratching bypasses Key Lock. Original two-deck layout remains available in Setup.
- Compact desktop layout: two scrolling waveforms, two vector vinyl jogs, two three-deck strip groups, four hot cues and four favorite sampler pads per side. Expanded mode exposes eight hot cues and nine sampler pads. Favorite assignments survive switching layouts. Auxiliary Cue Decks remain separate.
- Visible master BPM, quantization and Sync All; per-deck controls, stems/EQ, mix-duration stepper with saved timing, resizable library, amber A / cyan B, and purple sampler outlines.
- Waveform peak buffers are shared across views/decks of the same file, including in-flight decode coalescing. No analysis or separation is performed on the audio callback. The header displays actual audio envelopes, not simulated spectrum activity.
- Six-deck paused session recovery including focus, pair, paths, positions, gain, pitch, EQ, filter, sync, Key Lock and stem levels. Legacy two-deck checkpoints load without losing their original data.
- Regular and Smart Playlist opt-in for Companion. Authenticated scoped playlist/folder/search/paged discovery. Analyzed audio uses preparation fingerprints as resource IDs; legacy command IDs continue to work.
- Official Companion SDK module, scoped song/cue Learn pickers, 24 transport/LIVE presets, actual feedback, deck/sampler/FX/energy/scene/output controls, request IDs, session fences and reconnect without replaying transport commands.
- Device-local music scenes store playlist/style/energy/fade/master-clock settings in one local database file. Recall does not change the pair or stop background layers.
- Google Cast discovery, stable receiver selection, receiver-confirmed state and volume. Live broadcast has an independent level/mute; stopping broadcast leaves local playback alone.
- Independent Cast station playlist launch and revision-checked runtime energy/style/progression, applied at the next song selection without restarting the current station audio.
- Android retains two-deck Remote and offline playback. Added desktop sync/Key Lock/loop/stem tools and desktop Google speaker controls. No analysis engine was added to Android. Pixel installed development version: 0.2.0-dev, code 20.

## Verification completed locally

- Workspace Rust tests and warning-free Clippy; player: 129 passed, 31 hardware/codec-specific tests left ignored by the default run.
- Explicit synthetic six-stream Key Lock benchmark: five seconds of audio rendered in about 1.87 seconds, zero buffer underruns in that test.
- Measured sine test: 116% playback corrected from approximately 510 Hz back to 440 Hz.
- Real native desktop process with six generated WAV decoders: all voices play, LIVE changes preserve routing, unsafe pair changes reject, independent background fade leaves other voices playing, scoped resources return only the requested page, and emergency stop works.
- Pixel 7: development APK and test APK installed; Remote command/cue-deck/retry protocol test passed against an isolated desktop through USB forwarding. This is not a claim of a new end-to-end Wi-Fi/Google speaker acceptance pass.
- Companion: four automated client/preset/action contract tests and official SDK package generation pass. The packaged module is `integrations/companion/ampverse-0.2.0.tgz`.
- Actual application screenshots captured at 1570×1000, 1000×700 and 1920×1080, compact/expanded, Classic/Modern Dark, including 125% application scaling. Native maximize held at 2560×1552; restore and native resize held at 1429×1000 without rubberbanding.

Screenshots and local logs are under `test-output/six-deck` and are excluded from Git. CI artifacts will provide shareable captures and platform binaries after the branch checks run.

## Limits and acceptance work

- Android and Companion GitHub jobs passed for the first branch checkpoint. Linux compiled and passed unit tests/lint; follow-up CI fixes correct stale JSON-path smoke tests. Windows codec tests now run explicitly where FFmpeg is available rather than assuming every runner includes it.
- No actual Companion host is installed/running on this machine. SDK packaging, contract tests and protocol tests do not replace a real host button/feedback acceptance pass.
- The pitch backend adds 2176 frames (45.3 ms at 48 kHz) of latency before device buffering. Dry, pitch-locked and scratch paths share this delay; an impulse test confirms matching sample positions. Section handoffs retain processor history and EOF drains its delayed tail. Acoustic quality/latency acceptance remains required before a performance release. `AMP_DISABLE_KEYLOCK=1` disables the backend on slower systems; missing Windows DLL disables Key Lock rather than silently claiming pitch preservation.
- `amp-keylock.dll` and the bundled MIT notices must ship beside the Windows player. Packaging/install scripts now include them. Linux and MSVC CI acceptance must pass before distributing those builds.
- Fingerprint-based identity is rename-independent only while the corresponding preparation fingerprint remains associated with the audio. Unanalyzed tracks retain legacy path IDs; automatic recovery of preparation after arbitrary external file moves remains an edge case to verify.
- Synthetic throughput is not a complete real-world CPU/RAM/latency benchmark. Prepared-stem/multi-loop/FX/Auto-DJ-with-background load and sustained network activity still require the full performance matrix.
- New Google receiver control is not yet retested audibly on the selected physical speaker. Earlier production Cast tests are documented separately in `CAST_REMOTE_ACCEPTANCE.md` and are not counted as new-branch verification.
- Current Player transport still shares its original main-deck playback architecture. A fully independent additional Player voice and dedicated Stop Player behavior are not implemented by this six-deck change.

Production installation/release remains gated on these acceptance items. No claim that the entire supplied specification is finished should be inferred from the passing unit tests.
