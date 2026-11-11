# Mixxx MIDI mapping import

Desktop Library → Devices → DJ controllers → Import Mixxx mapping. Choose a `.midi.xml` file, expand the compatibility report, then find MIDI devices, choose the controller's MIDI input and press Connect supported controls. Disconnect before changing devices; reconnect after unplugging. Nothing connects automatically on startup. Saved imports live in the local `controllers.ampdb` file beside the player's settings, outside the shared music catalog.

This is partial input-format compatibility, not the complete Mixxx controller runtime. Import accepts a single controller with schema 1 in either `MixxxMIDIPreset` or `MixxxControllerPreset` XML. It imports only recognized direct assignments and shows the skipped entries. XML is size/node bounded and DTDs/external entities are rejected. It does not evaluate JavaScript or read referenced script paths. Ambiguous duplicate MIDI assignments are skipped together. HID, SysEx, LED/output feedback, relative encoders, 14-bit paired CCs, deck layers, scripted jog wheels and unknown options need separate AMP adapters. Ordinary pitch-bend messages support their native 14-bit value. Mappings that use scripts may therefore have very few compatible controls.

Supported direct controls:

- `[Master] crossfader`.
- `[Channel1]` / `[Channel2]`: `play` (toggle), `stop`, `cue_goto` (jump while retaining play/pause), `beatsync` / `beatsync_tempo` (AMP tempo matching only), `beatloop_4_toggle`, `volume`, `filterLow`, `filterMid`, `filterHigh`, `rate`, `pfl`.
- Normal, Button, Invert (continuous controls), and Soft-takeover options. Unknown options cause that input to be skipped. Cue-default/hold-preview and hot-cue editing are not translated to a different action.

Faders/knobs always use pickup: move the physical control close to or across its on-screen value. GUI changes re-arm pickup. Hardware transport or a picked-up mixer control takes over from Auto DJ; headphone routing alone leaves automation running. Physical controls remain manual controls under Manual DJ Lock. Disconnecting retains playback and current levels. The importer does not change cues, playlist rules, source audio or shared preparation.

Ranges follow AMP: channel gain 0–1, EQ -24–0–12 dB, and tempo +/-16% with low MIDI values faster. Invert reverses the hardware axis. These ranges/curves are not a reproduction of Mixxx's configurable mixer curves, rate orientation/ranges, phase sync or key lock. Configure private headphone output in the DJ mixer before using PFL.

Format references: [Mixxx MIDI XML specification](https://github.com/mixxxdj/mixxx/wiki/MIDI-controller-mapping-file-format), [Mixxx controls](https://manual.mixxx.org/2.5/en/chapters/appendix/mixxx_controls.html), and [official mapping directory](https://github.com/mixxxdj/mixxx/tree/main/res/controllers). The application links to that directory; it does not redistribute its mapping collection or script runtime. Keep original mapping authorship and licensing with files you distribute. AMP's importer and tests are independently written against the documented format. Mixxx is a separate GPL-licensed project; importing a file does not claim full hardware certification or affiliation.

Verification covers format validation, skipped scripts, malformed input, duplicate assignments, channel routing, note release, pitch-bend resolution, pickup/re-pickup and synthetic MIDI-to-player dispatch. Native port discovery is a separate smoke check. No physical controller has been available for button/knob/jog testing.
