# Live DJ stem controls

The center mixer fills the deck section. In short windows, channel controls scroll independently while the crossfader and stem controls remain below them. Below the crossfader, each deck always shows three knobs: Vocals, Instruments and Beat (drums). Center keeps the original level; turn left to reduce/remove that part, or right to reduce the other parts and isolate it. Double-click or Home resets a knob. Reset restores the original three-part mix. A uses amber and B uses cyan.

Load a song and press Prepare stems in its mixer column. Separation runs locally in a separate process, uses a compatible CUDA GPU automatically, and can be canceled. Quiet, Balanced and Fast select bounded CPU budgets and spacing between model segments. Only one job runs per application at a time. It never uploads music. When ready, pause the deck and press Load prepared stems, or let the prepared song load on a future transition. A playing deck keeps its snapshot. The three controls become active for prepared audio; they cannot separate an unprepared song instantly.

Vocals, drums and residual instruments share one six-channel decoder timeline, pitch, scratch history, loop and cue range. EQ, filters, crossfader and headphones process the result normally. Stem controls remain usable during AutoMix for prepared tracks. Ordinary songs still play without the add-on.

Prepared stems are a device-local cache, not NAS metadata or Android downloads. The cache tracks source path, size and modification time and checks all three output hashes before publication. Old two-part caches need preparation again for the drums part; their previous generation is retained. No user cue, beatgrid, rating or playlist is changed. Library-wide preparation is available under Analyze Music / Prepare Stems.

Demucs 4.0.1 with the official htdemucs checkpoint extracts vocals and drums. Instruments are the original minus both, preserving reconstruction at centered knobs. Quality depends on the recording and may contain bleed/artifacts. This is prepared separation, not zero-latency inference. Songs from 0.25 seconds to 30 minutes are supported.

VirtualDJ reference behavior: https://virtualdj.com/manuals/virtualdj/interface/mixer/audio.html . AMP implements its own controls and uses Demucs separation, not VirtualDJ code or models.

## Optional Windows add-on

From a source checkout, run `tools/install-stem-addon.ps1 -Python <Python-3.12-executable> -DownloadModel`. It installs a separate virtual environment under the player add-ons directory, CPU PyTorch/Torchaudio 2.6.0 and Demucs 4.0.1. No ML dependency is added to basic player playback or the BPM analysis add-on. The model is downloaded from Meta's official model host, with Demucs' checkpoint signature check. The initial model is about 80 MB; the runtime additionally requires several hundred MB. Pip dependencies retain their distribution license files. The installation script does not bundle them into the player executable.

On Linux, create an equivalent Python 3.12 environment with those versions and set `AMP_STEM_PYTHON` to its Python executable. FFmpeg must be available to the player and worker. `TORCH_HOME` is placed inside the add-on so its model cache is independent from other Python installations.

Primary upstream documentation: https://github.com/facebookresearch/demucs .

## Per-song defaults in Library

Select a song in Library / Edit song / Default stem mix for this song. Prepare its stems, adjust Vocals / Instruments / Beat, and use Preview stem mix to listen. Left removes that part; right isolates it. Original mix resets all three. Press Save approved metadata to publish the default.

Defaults use the existing revision and conflict checks and travel in the song's ampdj metadata. DJ applies them on the next load, including Auto DJ; playing decks keep their loaded snapshot. Songs without defaults load the original mix. Prepared audio is currently a local cache: another computer must prepare stems before playing a custom mix. Missing stems produce a preparation error instead of silently playing the original. Source audio is unchanged.

## Preparation jobs and added sources

Analyze library and Prepare library stems enumerate all configured roots and subfolders independently from the slower tag/preparation refresh. Job inputs contain paths, not replacement playback metadata. Analysis workers reload preparation in the owning library under the existing shared coordination rules. A disconnected source produces a visible error instead of silently excluding that source. Ordinary browsing retains its existing offline behavior.

Library stem preparation continues past individual failed files and reports the failure count and last error. Stop retains completed stems. Initial file enumeration on a large NAS may still take time; the interface shows that it is finding songs before the per-song counter begins.

If a stem environment is damaged, rerun the existing installation script with a working Python 3.12 interpreter. It repairs the environment in place while retaining installed packages and models. Verification on November 12 used the installed engine protocol round trip and a real Demucs separation/reconstruction test on isolated audio; user music was not changed.

## Preparation speed and playback priority (V1.0.1)

Library → Analyze Music has **Quiet / Balanced / Fast** buttons for both analysis and stems. This is one shared, remembered preference. Balanced is the default. Analysis adjusts concurrency while a batch is running. Stem CPU limits change with the next song; every mode uses the same separation model and quality settings.

Quiet uses one analysis song and one stem CPU thread, with longer spacing between GPU segments. Balanced uses up to a quarter of logical CPUs, capped at four analysis songs/four stem CPU threads. Fast permits up to half of logical CPUs (maximum twelve analysis songs) and a third for stem CPU threads (maximum eight). One stem song is separated at a time to bound memory use.

Playback takes priority in every mode, including manual decks, Auto DJ, cue decks, samplers, and live broadcasting. New preparation waits; running analysis pauses between audio blocks and stem inference pauses between model segments. A decoder/import or current inference segment may finish before the next checkpoint. No preparation changes live playback snapshots. Analysis lease renewal continues during the pause. Preparation resumes automatically after playback stops. The expiring process-local pause marker prevents a crashed player from permanently freezing a worker.

Analysis stages working audio in a temporary local work-cache directory. Stem jobs already stage their input locally. Completed results are reused, source files stay unchanged, temporary analysis files are removed, and stem batches continue past failed songs.

### NVIDIA GPU setup

For a compatible NVIDIA GPU, use `tools/install-stem-addon.ps1 -Python <Python-3.12-executable> -Destination <player-folder>/addons/stems -Gpu -DownloadModel`. It selects the official pinned PyTorch/Torchaudio 2.6.0 CUDA 12.4 wheels. No driver or system Python is changed. Without `-Gpu`, the installer retains the CPU-only option. The runtime checks CUDA availability asynchronously and shows the detected GPU or CPU fallback in Prepare Stems. If GPU separation fails, it retries that song on CPU and retains the failure summary if that also fails.

Official compatibility/install sources: [PyTorch previous versions](https://docs.pytorch.org/get-started/previous-versions/) and [CUDA availability](https://docs.pytorch.org/docs/main/generated/torch.cuda.is_available.html). The optional runtime contains its upstream distribution notices. It is not embedded in the base player package.

Local verification used an RTX 3080 Ti Laptop GPU: real separation paused mid-job and resumed, produced checked three-part outputs, and preserved the original audio. The real analysis engine was checked for pause/resume, completed-work reuse and temporary-cache cleanup. These checks do not establish universal hardware latency guarantees.

V1.0.1 validation: 312 workspace tests passed (53 hardware/network cases skipped), plus three real preparation integration checks and two opted-in native checks. Installed-player verification loaded the saved Quiet preference, started a muted DJ deck, observed preparation pause while the playhead advanced, recorded zero underruns, and observed preparation resume after pausing the deck. The Windows V1 preview includes the matching rebuilt analysis executable; update the player and analysis add-on together for cooperative pause support.
