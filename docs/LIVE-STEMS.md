# Live stems

DJ stem controls are armed when the live-stem module is installed. There is no Enable step for newly loaded DJ songs. All three knobs at **0** means no stem filtering and no separation work: the deck plays the original audio. Move Vocals, Instruments or Beat away from 0 to start separation in the background. Original audio continues on the same timeline during warm-up, then blends into the filtered result. **Right-click**, double-click or Home resets a knob to 0; resetting every knob stops separation and releases the processing slot. Each deck keeps its own filters across Auto Mix song changes. A reset does not remove the track or stop its playback.

Normal playback supports all six main DJ decks. Live separation has a separate conservative limit: choose a maximum of **2, 4 or 6** in a paused deck's Stems control. The preference defaults to four, clamped to two on CPU or unqualified hardware. A verified qualified CUDA module plus a matching local hardware qualification receipt permits four by default and six only after explicitly choosing six. The receipt binds host, adapter UUID, worker hash, pinned model hash, tested tier and proof source/run. Copying a module to a different or weaker GPU does not copy its qualification; missing/mismatched receipts remain at two until that machine is tested. Receipt validation happens in the background without starting the model. Core count alone never qualifies a higher tier. Choosing two or four on qualified GPU hardware is also supported. The displayed limit applies to decks requesting non-neutral live filters, not idle armed decks. A deck that cannot obtain a processing slot continues original playback and reports the limit. Lowering below the number of requested live decks is refused; reset some decks to neutral first. Pending inference retains its slot until cancellation is acknowledged.

Optional local preparation applies only to a selected regular or saved Smart Playlist, never a whole-library batch. In Library > Playlists or Smart Playlists, select a saved playlist and choose **Prepare selected playlist stems…**. Review its name, unique song count and estimated size, then explicitly start. Smart rules are resolved into a fixed song snapshot at confirmation; save/reload unsaved Smart edits first. Progress and Cancel remain on the playlist page. Reset live-stem filters to neutral before starting preparation. Neutral DJ playback remains available during preparation; new separation requests wait or report unavailable capacity while original audio continues.

The cache stores three gapless MP3 parts at256kbps each, with gain-restoration metadata, source/model identities, exact frame counts and file hashes. Total storage is capped at10GiB, each song at250MiB/two hours, with no automatic library expansion. Completed songs remain usable after cancellation. Pause a cached song's deck and choose **Enable prepared stems**; verified cached parts enter the same independent stem controls without consuming a live-separation slot. Up to six prepared/original decks remain available on weaker hardware. Clear local prepared stems is explicit and blocked while prepared decks are enabled; switch them to original audio first. Cache integrity is checked before playback and invalid entries fall back to original audio with a warning. Original songs are never overwritten. Live processing stays RAM-only and the legacy unbounded disk-writing separator remains disabled.

Processing looks ahead in 11-second windows with overlap joins and bounded 16-second playback rings. Source decoding, separated samples and overlap history remain in RAM. No prepared-stem audio, compressed stem audio or per-song audio database is written. Existing analysis catalogues retain metadata and cue/section hints; compact metadata cannot replace source-separation inference. Ordinary Player playback does not start stem processing.

The native CPU module in `addons/stems-live` remains about 30 MB and contains no Python environment. The optional `addons/stems-cuda` runtime is roughly 2.1 GiB of fixed NVIDIA/ONNX software and license notices, shared by all songs and decks. It is separate from the base update and an unchanged verified runtime is reused. The old disk-writing separator remains withdrawn; its historical instructions are not current installation instructions.

CPU workers share a configured maximum of 6 GiB of process-memory budgets. The CUDA worker needs a 6 GiB Windows process-commit allowance even though measured resident usage can be much lower. Its 4 GiB CUDA arena setting is not a hard total GPU-VRAM cap. Disk footprint, process commit, resident RAM and GPU VRAM are different measurements. Missing or failed separation reports a warning and preserves original audio rather than writing a disk cache.

The normal CI helper verifies actual single/dual CPU pipelines and deterministic six-deck mixer/filter routing. Physical six-deck CUDA qualification is explicit: `python tools/verify_live_stem_player.py --module <verified-native-base> --six-decks --cuda-module <verified-CUDA-runtime> --cuda-manifest-sha256 <pinned-manifest-hash>`. It requires six actual model-backed decks at 116% speed for 90 seconds, independent filters, then five surviving decks for eight seconds after cancellation; it checks clocks, underruns, fallback, process quotas and unchanged storage. GPU qualification is not reported when that opt-in test did not run. Reconstructing the original mix validates routing and joins, not subjective vocal-separation quality.

## Opt-in sustained CPU2 qualification

Ordinary CI retains the 30-second two-deck run plus six seconds with B surviving A's cancellation. The test-only `AMP_STEM_TEST_CPU_PROFILE` accepts only `short` (default) or `sustained` (180 seconds together plus 15 survivor seconds). Unknown values fail. Source requirements are 45 and 210 seconds respectively; these do not change production mixing or DSP.

For a sustained run against a verified packaged native module, use the full-release **test executable** compiled from this test source, not the normal desktop executable:

Linux:

```sh
python3 tools/verify_live_stem_player.py --module /absolute/path/to/verified-module --sustained-cpu --test-binary /absolute/path/to/full-release-amp-player-test --log-dir /absolute/path/to/new-cpu2-evidence
```

Windows PowerShell:

```powershell
py tools/verify_live_stem_player.py --module 'D:\path\to\verified-module' --sustained-cpu --test-binary 'D:\path\to\full-release-amp-player-test.exe' --log-dir 'D:\path\to\new-cpu2-evidence'
```

Without `--test-binary`, sustained mode invokes Cargo's full release profile; ordinary mode keeps the existing Cargo profile. Both modes still run the single-source pipeline, distinct-filter/cancellation checks, deterministic six-deck controls and native selected-playlist preparation. `--six-decks` remains a separate explicit CUDA qualification and still requires its pinned bundle options; adding sustained CPU mode never skips or qualifies GPU work.

The helper generates 220 seconds of original synthetic input in sustained mode. `--source-seconds` can choose a bounded duration (at most 600 seconds), but rejects anything shorter than the selected tests require. It records actual module/worker/model/runtime hashes, per-test logs and a success or failure receipt in a new/empty evidence directory. An exact test must actually execute one passing test; a missing test name is rejected. The CPU test's reported duration must match the requested policy, so an older short-only test binary cannot be labelled sustained. Binary source provenance must still be independently verified.

Native tests retain their underrun/fallback, source-integrity, cancellation and worker-cleanup assertions; generated input is removed afterward. Synthetic model/callback acceptance is not physical audio-device or real-music listening acceptance, and is not four/six-deck qualification. The earlier Linux proof at f00 plus test-only a121 remains historical evidence of that exact source; these options do not relabel it.

Offline command/configuration checks (no model or build):

```sh
python3 -m unittest discover -s tools -p test_verify_live_stem_player.py -v
```
