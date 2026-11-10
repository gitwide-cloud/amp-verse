# Live DJ stem controls

The center mixer fills the deck section. In short windows, channel controls scroll independently while the crossfader and stem controls remain below them. Below the crossfader, each deck always shows three knobs: Vocals, Instruments and Beat (drums). Center keeps the original level; turn left to reduce/remove that part, or right to reduce the other parts and isolate it. Double-click or Home resets a knob. Reset restores the original three-part mix. A uses amber and B uses cyan.

Load a song and press Prepare stems in its mixer column. Separation runs locally in a separate process, with four CPU threads at most, and can be canceled. Only one job runs per application at a time. It never uploads music. When ready, pause the deck and press Load prepared stems, or let the prepared song load on a future transition. A playing deck keeps its snapshot. The three controls become active for prepared audio; they cannot separate an unprepared song instantly.

Vocals, drums and residual instruments share one six-channel decoder timeline, pitch, scratch history, loop and cue range. EQ, filters, crossfader and headphones process the result normally. Stem controls remain usable during AutoMix for prepared tracks. Ordinary songs still play without the add-on.

Prepared stems are a device-local cache, not NAS metadata or Android downloads. The cache tracks source path, size and modification time and checks all three output hashes before publication. Old two-part caches need preparation again for the drums part; their previous generation is retained. No user cue, beatgrid, rating or playlist is changed. Library-wide automatic separation is not enabled.

Demucs 4.0.1 with the official htdemucs checkpoint extracts vocals and drums. Instruments are the original minus both, preserving reconstruction at centered knobs. Quality depends on the recording and may contain bleed/artifacts. This is prepared separation, not zero-latency inference. Songs from 0.25 seconds to 30 minutes are supported.

VirtualDJ reference behavior: https://virtualdj.com/manuals/virtualdj/interface/mixer/audio.html . AMP implements its own controls and uses Demucs separation, not VirtualDJ code or models.

## Optional Windows add-on

From a source checkout, run `tools/install-stem-addon.ps1 -Python <Python-3.12-executable> -DownloadModel`. It installs a separate virtual environment under the player add-ons directory, CPU PyTorch/Torchaudio 2.6.0 and Demucs 4.0.1. No ML dependency is added to basic player playback or the BPM analysis add-on. The model is downloaded from Meta's official model host, with Demucs' checkpoint signature check. The initial model is about 80 MB; the runtime additionally requires several hundred MB. Pip dependencies retain their distribution license files. The installation script does not bundle them into the player executable.

On Linux, create an equivalent Python 3.12 environment with those versions and set `AMP_STEM_PYTHON` to its Python executable. FFmpeg must be available to the player and worker. `TORCH_HOME` is placed inside the add-on so its model cache is independent from other Python installations.

Primary upstream documentation: https://github.com/facebookresearch/demucs .
