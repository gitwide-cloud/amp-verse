# Native stem worker provenance

This crate contains the bounded spectral adapter and native ONNX Runtime host. It does not bundle, download or execute Python, Torch or arbitrary checkpoints. Install the runtime and exact model separately; packaging is responsible for including the applicable license texts.

SCNet: SCNet authors, https://github.com/starrytong/SCNet (MIT). The author explicitly confirmed MIT licensing of released SCNet/SCNet-large pretrained weights and permission for attributed ONNX redistribution: https://github.com/starrytong/SCNet/issues/35 (author reply July17,2026).

Accepted converted/quantized model: https://huggingface.co/square-zero-labs/scnet/tree/af9bf78fdf676109a67211b32a3b54a1069acd40 ; revision af9bf78fdf676109a67211b32a3b54a1069acd40, 12900277bytes, SHA256 98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845. Original model and conversion attribution must remain with any redistributed model. No model is included by this crate.

Spectral contract reference: audiojs/neural-separate (MIT), https://github.com/audiojs/neural-separate ; SCNet uses normalized4096-point FFT, hop1024, center reflection, rectangular overlap/add. The adapter implements standard FFT/OLA directly; tests verify roundtrip and directDFT bins. This is actual source estimation when the separately installed model is used, not a vocal-presence timeline.

ONNX Runtime: Microsoft, MIT, https://github.com/microsoft/onnxruntime . Loaded dynamically via ort (https://github.com/pykeio/ort, MIT/Apache-2.0). Build disables ort's default/download features. realfft/rustfft (https://github.com/HEnquist/realfft ; https://github.com/ejmahler/RustFFT), MIT/Apache-2.0. Distribution must carry license texts for bundled dependencies/runtime/model.
