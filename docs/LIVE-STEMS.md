# Live stems

Load a local song into AMP DJ, pause its deck, open **Stems**, then choose **Enable live stems**. After initial processing finishes, play the deck and adjust Vocals, Instruments or Beat. Pause and choose **Use original audio** to return to the source mix. Saved song settings remain intact.

The CPU engine supports up to two live decks and device output rates up to 48 kHz. It looks ahead in 11-second source windows, with overlap joins and a 16-second playback ring per deck. Loading or seeking can require a few seconds of processing. Ordinary Player playback does not start stem processing.

Source decoding, model output and overlap buffers stay in RAM. This implementation does not write prepared-stem audio, compressed stem files, or audio databases. The pinned model and native runtime are shared engine files. Worker failure restores the original mix and reports a warning.

The installed addons/stems-live module contains a native worker, pinned SCNet model, CPU ONNX Runtime, licenses and an integrity manifest. It contains no Python environment. The old disk-writing separator remains withdrawn.

The initial implementation uses CPU processing with a 3 GiB worker-process memory limit. Engine installation size and active processing RAM are different measurements. A small song metadata map cannot replace the separation model.

The acceptance tests check actual inference, throughput, cancellation, overlap continuity and unchanged storage. Reconstructing the original mix validates routing and joins, but does not establish subjective separation quality.
