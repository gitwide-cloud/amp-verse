# Lab to sampler

Open AMP Lab, open an audio file (or choose From Library), and set the waveform selection's IN and OUT. Samples can be up to 30 seconds.

In **Quick sample editor**, enter a name, choose Sampler A or B and pad 1–9, and optionally enable Loop. For an isolated sample, enable **Isolate stems** and select Vocals, Instruments, Beat, or a combination. The optional installed Demucs add-on separates only the selected excerpt locally. Preview Sample auditions the processed excerpt; Stop Preview stops it. **Export to sampler** renders a new WAV and queues it directly into the chosen pad. A playing pad is replaced after it stops.

The exported sample includes the selection, gain, fades, normalization/limiting and selected stems. Source music and its cue metadata remain unchanged. Samples are saved in the music root's `ampdj/samples`; without a configured library, the fallback is `Documents/ampdj/samples`. Preview files are removed when preview stops or Lab closes. Sampler assignments, names, colors, levels, loop and toggle settings persist on this computer.

Right-click a DJ sampler pad to rename, choose its color, adjust level, enable loop, or enable toggle (press again to stop). With toggle disabled, repeated triggers restart the sample from its beginning. These controls use the existing two nine-pad banks; this update does not add VirtualDJ's video sampler, tempo-locked playback, or unlimited banks.
