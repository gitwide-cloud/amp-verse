"""Optional Demucs worker. Source music is read-only; playback has priority."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def pause_active(path, now=None):
    try:
        now = time.time() * 1000 if now is None else now
        until = int(Path(path).read_text().strip())
        return now < until <= now + 10000
    except (OSError, ValueError, TypeError):
        return False


def checkpoint():
    path = os.environ.get("AMP_PREPARATION_PAUSE_FILE")
    if path and pause_active(path):
        print("Paused for playback", flush=True)
        while pause_active(path):
            time.sleep(0.1)
        print("Resuming preparation", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", nargs="?")
    parser.add_argument("destination", nargs="?")
    parser.add_argument("--preload", action="store_true")
    parser.add_argument("--probe", action="store_true")
    args = parser.parse_args()
    checkpoint()
    import numpy as np
    import torch
    import demucs.apply as demucs_apply
    from demucs.pretrained import get_model
    torch.set_num_threads(max(1, min(8, int(os.environ.get("AMP_STEM_THREADS", "4")))))
    torch.set_num_interop_threads(1)
    cuda = torch.cuda.is_available()
    device = "cuda" if cuda else "cpu"
    if args.probe:
        print(json.dumps({"device": device, "name": torch.cuda.get_device_name(0) if cuda else "CPU", "cuda_build": torch.version.cuda}))
        return
    checkpoint()
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
        return
    # Demucs recursively applies the model to short, overlapping segments.
    # Check each segment without blocking the GUI or modifying live audio buffers.
    original_apply = demucs_apply.apply_model
    def guarded_apply(*a, **kw):
        checkpoint()
        time.sleep(max(0, min(1000, int(os.environ.get("AMP_STEM_IDLE_MS", "30")))) / 1000)
        return original_apply(*a, **kw)
    demucs_apply.apply_model = guarded_apply
    ffmpeg = os.environ.get("AMP_STEM_FFMPEG", "ffmpeg")
    rate = model.samplerate
    checkpoint()
    raw = subprocess.run([ffmpeg, "-v", "error", "-threads", "1", "-i", args.source, "-t", "1801", "-ac", "2", "-ar", str(rate), "-f", "f32le", "pipe:1"], check=True, capture_output=True).stdout
    samples = np.frombuffer(raw, dtype="<f4").reshape(-1, 2).copy()
    if len(samples) < rate // 4 or len(samples) > rate * 1800 or not np.isfinite(samples).all():
        raise RuntimeError("Prepare a valid song between 0.25 seconds and 30 minutes")
    original = torch.from_numpy(samples.T.copy())
    reference = original.mean(0)
    scale = reference.std().clamp_min(1e-7)
    mean = reference.mean()
    print("Separating on " + (torch.cuda.get_device_name(0) if cuda else "CPU"), flush=True)
    checkpoint()
    with torch.inference_mode():
        try:
            separated = guarded_apply(model, ((original - mean) / scale)[None], device=device, shifts=0, split=True, overlap=0.25, progress=True, num_workers=0)[0].cpu()
        except RuntimeError as error:
            if device != "cuda":
                raise
            print("GPU unavailable for this song; retrying on CPU: " + str(error), flush=True)
            device = "cpu"
            model.cpu()
            torch.cuda.empty_cache()
            checkpoint()
            separated = guarded_apply(model, ((original - mean) / scale)[None], device="cpu", shifts=0, split=True, overlap=0.25, progress=True, num_workers=0)[0].cpu()
    checkpoint()
    vocals = separated[model.sources.index("vocals")] * scale + mean
    drums = separated[model.sources.index("drums")] * scale + mean
    instruments = original - vocals - drums
    out = Path(args.destination)
    out.mkdir(parents=True, exist_ok=True)
    manifest = {"engine": "demucs-4.0.1/htdemucs-3stem", "device": device, "sample_rate": rate, "frames": original.shape[1], "channels": 2, "files": {}}
    for name, audio in [("vocals", vocals), ("instrumental", instruments), ("drums", drums)]:
        checkpoint()
        values = audio.cpu().numpy().T.astype("<f4")
        if not np.isfinite(values).all():
            raise RuntimeError("Invalid separated samples")
        target = out / (name + ".wav")
        subprocess.run([ffmpeg, "-v", "error", "-threads", "1", "-y", "-f", "f32le", "-ar", str(rate), "-ac", "2", "-i", "pipe:0", "-c:a", "pcm_f32le", str(target)], input=values.tobytes(), check=True)
        manifest["files"][name] = {"sha256": hashlib.sha256(target.read_bytes()).hexdigest(), "bytes": target.stat().st_size}
    (out / "audio.json").write_text(json.dumps(manifest), encoding="utf-8")
    print("Vocals, instruments and drums ready", flush=True)


if __name__ == "__main__":
    main()
