# Vocal and instrumental stems

Load a song in AMP DJ and expand **Stems · Vocals / Instrumental** on its deck. Press **Prepare stems**. Separation runs locally in a separate process, with four CPU threads at most, and can be canceled. It never uploads music. Only one preparation job runs per application at a time.

When preparation finishes, pause the deck and press **Load prepared stems**, or let the prepared song load on a future transition. An already playing deck keeps its original snapshot. Each separated component has Level, Mute and Solo; Reset restores both. Vocals and instrumental share one decoder timeline, pitch, scratch history, loop and cue range. The mixer EQ, filters, crossfader and headphone bus process the resulting mix normally. Ordinary songs still play without this add-on.

Prepared stems are currently a device-local cache beside the app preferences, not NAS metadata or Android offline downloads. The cache is keyed by source path, size and modification time; completed generations contain an audio hash and hashes of both prepared files. Temporary work is published only after validation. Source changes invalidate the local cache. Existing corrupt generations are retained separately when a new generation is prepared. No user cue, beatgrid, rating or playlist is changed. Library-wide or automatic separation is not enabled.

The optional separator is Demucs 4.0.1 with the official `htdemucs` checkpoint. It extracts vocals and creates a phase-aligned instrumental residual, preserving original mix reconstruction when both levels are full. Separation quality depends on the recording and can include bleed or artifacts. This is prepared separation, not a zero-latency AI process in the live audio callback. Files from 0.25 seconds to 30 minutes are supported by this initial worker.

## Optional Windows add-on

From a source checkout, run `tools/install-stem-addon.ps1 -Python <Python-3.12-executable> -DownloadModel`. It installs a separate virtual environment under the player add-ons directory, CPU PyTorch/Torchaudio 2.6.0 and Demucs 4.0.1. No ML dependency is added to basic player playback or the BPM analysis add-on. The model is downloaded from Meta's official model host, with Demucs' checkpoint signature check. The initial model is about 80 MB; the runtime additionally requires several hundred MB. Pip dependencies retain their distribution license files. The installation script does not bundle them into the player executable.

On Linux, create an equivalent Python 3.12 environment with those versions and set `AMP_STEM_PYTHON` to its Python executable. FFmpeg must be available to the player and worker. `TORCH_HOME` is placed inside the add-on so its model cache is independent from other Python installations.

Primary upstream documentation: https://github.com/facebookresearch/demucs .
