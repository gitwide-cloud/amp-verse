"""Build-only pinned native CPU module packager. Python is NOT included in its output.
No audio files, package installation, activation or owner-library writes.
"""
import argparse
from dataclasses import dataclass
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MIN_MODULE_BYTES = 0
TARGET_MODULE_BYTES = 100_000_000
MAX_MODULE_BYTES = 512_000_000

@dataclass(frozen=True)
class Asset:
    url: str
    size: int
    sha256: str

REVISION = "af9bf78fdf676109a67211b32a3b54a1069acd40"
MODEL = Asset(f"https://huggingface.co/square-zero-labs/scnet/resolve/{REVISION}/scnet.int8.onnx", 12900277,
              "98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845")
MODEL_LICENSE = Asset(f"https://huggingface.co/square-zero-labs/scnet/resolve/{REVISION}/LICENSE", 1067,
                      "0bdf1b69335198118ab16cfc50d337b496b8c6d90e83beeaba4643781ab62513")
RUNTIMES = {
    "windows-x64": (Asset("https://files.pythonhosted.org/packages/36/b4/3f1c71ce1d3d21078a6a74c5483bfa2b07e41a8d2b8fb1e9993e6a26d8d3/onnxruntime-1.22.0-cp312-cp312-win_amd64.whl", 12692233,
                          "c0d534a43d1264d1273c2d4f00a5a588fa98d21117a3345b7104fa0bbcaadb9a"),
                    "onnxruntime/capi/onnxruntime.dll", "onnxruntime.dll", "amp-stem-worker.exe"),
    "linux-x64": (Asset("https://files.pythonhosted.org/packages/7a/d5/aa83d084d05bc8f6cf8b74b499c77431ffd6b7075c761ec48ec0c161a47f/onnxruntime-1.22.0-cp311-cp311-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl", 16393496,
                        "b978aa63a9a22095479c38371a9b359d4c15173cbb164eaad5f2cd27d666aa65"),
                  "onnxruntime/capi/libonnxruntime.so.1.22.0", "libonnxruntime.so", "amp-stem-worker"),
}

def checked(data, asset):
    if len(data) != asset.size or hashlib.sha256(data).hexdigest() != asset.sha256:
        raise ValueError("Pinned download size/SHA256 mismatch")
    return data


def fetch_asset(asset):
    if not asset.url.startswith("https://") or not 0 < asset.size <= 20_000_000:
        raise ValueError("Unexpected build asset URL/size")
    request = urllib.request.Request(asset.url, headers={"User-Agent": "AMP-Verse-native-module-builder/1"})
    with urllib.request.urlopen(request, timeout=90) as response:
        length = response.headers.get("Content-Length")
        if length is not None and int(length) != asset.size:
            raise ValueError("Pinned download Content-Length mismatch")
        data = response.read(asset.size + 1)
    return checked(data, asset)


def read_asset(path, asset):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_size != asset.size:
        raise ValueError("Pinned local build asset size mismatch")
    with path.open("rb") as stream:
        return checked(stream.read(asset.size + 1), asset)


def runtime_files(wheel, member, runtime_name):
    """Select exact known members; never extract an archive path or Python package."""
    wanted = {member: runtime_name, "onnxruntime/LICENSE": "LICENSES/ONNXRuntime-LICENSE.txt",
              "onnxruntime/ThirdPartyNotices.txt": "LICENSES/ONNXRuntime-ThirdPartyNotices.txt"}
    result = {}
    with zipfile.ZipFile(io.BytesIO(wheel)) as archive:
        for source, destination in wanted.items():
            entries = [entry for entry in archive.infolist() if entry.filename == source]
            if len(entries) != 1:
                raise ValueError(f"Runtime wheel missing/duplicate required member: {source}")
            entry = entries[0]
            if entry.is_dir() or stat.S_ISLNK(entry.external_attr >> 16) or not 0 < entry.file_size <= 40_000_000:
                raise ValueError("Invalid bounded runtime member")
            result[destination] = archive.read(entry)
    return result


def encode_manifest(payload):
    """Count manifest itself without attempting an impossible self-hash."""
    payload["manifest_bytes"] = 0
    payload["total_bytes"] = payload["payload_bytes"]
    for _ in range(10):
        data = (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode()
        if payload["manifest_bytes"] == len(data):
            return data
        payload["manifest_bytes"] = len(data)
        payload["total_bytes"] = payload["payload_bytes"] + len(data)
    raise ValueError("Manifest byte-count convergence failed")


def package(worker, output, target, model_source=None, runtime_wheel=None, fetch=fetch_asset):
    worker, output = Path(worker), Path(output)
    if output.exists() or output.is_symlink():
        raise ValueError("Module output already exists; choose a new build directory")
    if worker.is_symlink() or not worker.is_file() or not 0 < worker.stat().st_size <= 128_000_000:
        raise ValueError("A bounded release native worker is required")
    worker_bytes = worker.read_bytes()
    magic = b"MZ" if target == "windows-x64" else b"\x7fELF"
    if not worker_bytes.startswith(magic):
        raise ValueError("Worker format does not match requested target")
    runtime, member, runtime_name, worker_name = RUNTIMES[target]
    model = read_asset(model_source, MODEL) if model_source else checked(fetch(MODEL), MODEL)
    wheel = read_asset(runtime_wheel, runtime) if runtime_wheel else checked(fetch(runtime), runtime)
    model_license = checked(fetch(MODEL_LICENSE), MODEL_LICENSE)
    files = runtime_files(wheel, member, runtime_name)
    files.update({worker_name: worker_bytes, "scnet.int8.onnx": model,
                  "NOTICES.md": (ROOT / "crates/amp-stem-worker/NOTICES.md").read_bytes(),
                  "LICENSES/SCNet-MIT.txt": model_license,
                  "LICENSES/AMP-Verse-LICENSE.txt": (ROOT / "LICENSE").read_bytes()})
    entries = [{"path": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
               for name, data in sorted(files.items())]
    manifest = {"schema": 1, "module": "amp-live-stems", "target": target,
                "worker_protocol": 1, "provider": "cpu", "max_frames": 485100,
                "persistent_audio_bytes": 0, "python_runtime_included": False,
                "model_revision": REVISION, "model_sha256": MODEL.sha256,
                "runtime_version": "1.22.0", "runtime_archive_sha256": runtime.sha256,
                "relative_paths": {"worker": worker_name, "model": "scnet.int8.onnx", "runtime": runtime_name},
                "payload_files": entries, "file_count": len(entries) + 1,
                "payload_bytes": sum(entry["bytes"] for entry in entries),
                "distribution_target_bytes": TARGET_MODULE_BYTES, "within_distribution_target": True}
    manifest_data = encode_manifest(manifest)
    if manifest["total_bytes"] > TARGET_MODULE_BYTES:
        manifest["within_distribution_target"] = False
        manifest["size_warning"] = "Module exceeds the100MB distribution target; review the measured size."
        manifest_data = encode_manifest(manifest)
    if not MIN_MODULE_BYTES <= manifest["total_bytes"] <= MAX_MODULE_BYTES:
        raise ValueError("Module exceeds the absolute512MB build safety limit")
    output.parent.mkdir(parents=True, exist_ok=True)
    # Only this new private build staging directory is ever removed on failure.
    with tempfile.TemporaryDirectory(prefix=".amp-live-stems-", dir=output.parent) as temporary:
        stage = Path(temporary) / "module"
        stage.mkdir()
        for name, data in files.items():
            path = stage / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        if target == "linux-x64":
            (stage / worker_name).chmod(0o755)
            (stage / runtime_name).chmod(0o755)
        (stage / "module.json").write_bytes(manifest_data)
        actual = [p for p in stage.rglob("*") if p.is_file()]
        if sum(p.stat().st_size for p in actual) != manifest["total_bytes"] or len(actual) != manifest["file_count"]:
            raise ValueError("Module accounting mismatch")
        if output.exists():
            raise ValueError("Output appeared during packaging; refusing to overwrite")
        stage.rename(output)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--target", choices=RUNTIMES, required=True)
    parser.add_argument("--model-source", help="Optional already downloaded pinned model")
    parser.add_argument("--runtime-wheel", help="Optional already downloaded pinned CPU wheel")
    args = parser.parse_args()
    try:
        manifest = package(args.worker, args.output, args.target, args.model_source, args.runtime_wheel)
        print(json.dumps({key: manifest[key] for key in ("target", "total_bytes", "file_count", "persistent_audio_bytes", "within_distribution_target")}, sort_keys=True))
        if not manifest["within_distribution_target"]:
            print(manifest["size_warning"], file=sys.stderr)
    except (ValueError, OSError, zipfile.BadZipFile) as error:
        parser.exit(1, f"Native module packaging failed: {error}\n")

if __name__ == "__main__":
    main()
