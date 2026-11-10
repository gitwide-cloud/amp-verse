"""Optional local Demucs worker. Original music and preparation are read-only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("source", nargs="?")
parser.add_argument("destination", nargs="?")
parser.add_argument("--preload", action="store_true")
args = parser.parse_args()

import numpy as np
import torch
from demucs.apply import apply_model
from demucs.pretrained import get_model

torch.set_num_threads(max(1, min(4, (os.cpu_count() or 2) // 2)))
# Demucs packages contain a model description as well as tensors. Only its fixed,
# official, hash-checked pretrained checkpoint is opened here; never user pickle files.
original_load = torch.load
def trusted_model_load(*a, **kw):
    kw.setdefault("weights_only", False)
    return original_load(*a, **kw)
torch.load = trusted_model_load
try:
    model = get_model("htdemucs").cpu().eval()
finally:
    torch.load = original_load
if args.preload:
    print("Stem model ready", flush=True)
    raise SystemExit(0)

ffmpeg = os.environ.get("AMP_STEM_FFMPEG", "ffmpeg")
rate = model.samplerate
raw = subprocess.run([ffmpeg, "-v", "error", "-i", args.source, "-t", "1801", "-ac", "2", "-ar", str(rate), "-f", "f32le", "pipe:1"], check=True, capture_output=True).stdout
samples = np.frombuffer(raw, dtype="<f4").reshape(-1, 2).copy()
if len(samples) < rate // 4 or len(samples) > rate * 1800 or not np.isfinite(samples).all():
    raise RuntimeError("Prepare a valid song between 0.25 seconds and 30 minutes")
original = torch.from_numpy(samples.T.copy())
reference = original.mean(0)
scale = reference.std().clamp_min(1e-7)
mean = reference.mean()
print("Separating vocals, instruments and drums locally on CPU", flush=True)
with torch.inference_mode():
    separated = apply_model(model, ((original - mean) / scale)[None], device="cpu", shifts=0, split=True, overlap=0.25, progress=True, num_workers=0)[0]
vocals = separated[model.sources.index("vocals")] * scale + mean
# Residual accompaniment preserves original phase, loudness, and exact alignment.
drums = separated[model.sources.index("drums")] * scale + mean
instruments = original - vocals - drums
out = Path(args.destination)
out.mkdir(parents=True, exist_ok=True)
manifest = {"engine": "demucs-4.0.1/htdemucs-3stem", "sample_rate": rate, "frames": original.shape[1], "channels": 2, "files": {}}
for name, audio in [("vocals", vocals), ("instrumental", instruments), ("drums", drums)]:
    values = audio.cpu().numpy().T.astype("<f4")
    if not np.isfinite(values).all():
        raise RuntimeError("Invalid separated samples")
    target = out / (name + ".wav")
    subprocess.run([ffmpeg, "-v", "error", "-y", "-f", "f32le", "-ar", str(rate), "-ac", "2", "-i", "pipe:0", "-c:a", "pcm_f32le", str(target)], input=values.tobytes(), check=True)
    manifest["files"][name] = {"sha256": hashlib.sha256(target.read_bytes()).hexdigest(), "bytes": target.stat().st_size}
(out / "audio.json").write_text(json.dumps(manifest), encoding="utf-8")
print("Vocals, instruments and drums ready", flush=True)
