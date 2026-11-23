"""Integrity contract for the optional CUDA runtime; never stores song audio."""
import hashlib
import json
from pathlib import Path
import re
import stat
import struct

MODEL_SHA256 = "98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845"
MAX_BYTES = 3 * 1024**3
FILES = frozenset((
    "dependency-inventory.json", "NOTICES.md",
    "LICENSES/NVIDIA-CUDA-12.4.0-EULA.html",
    "LICENSES/NVIDIA-cuDNN-9.1.0-EULA.html",
    "LICENSES/NVIDIA-license-provenance.json",
    "LICENSES/ONNXRuntime-LICENSE.txt",
    "LICENSES/ONNXRuntime-ThirdPartyNotices.txt",
    "LICENSES/PyTorch-distribution-LICENSE.txt",
    "LICENSES/PyTorch-distribution-NOTICE.txt",
    "onnxruntime.dll", "onnxruntime_providers_cuda.dll",
    "onnxruntime_providers_shared.dll",
    "vendor/cublas64_12.dll", "vendor/cublasLt64_12.dll",
    "vendor/cudart64_12.dll", "vendor/cufft64_11.dll",
    "vendor/cudnn64_9.dll", "vendor/cudnn_adv64_9.dll",
    "vendor/cudnn_cnn64_9.dll", "vendor/cudnn_engines_precompiled64_9.dll",
    "vendor/cudnn_engines_runtime_compiled64_9.dll",
    "vendor/cudnn_graph64_9.dll", "vendor/cudnn_heuristic64_9.dll",
    "vendor/cudnn_ops64_9.dll",
))
OPTIONS = {
    "device_id": "0", "gpu_mem_limit": "4294967296",
    "arena_extend_strategy": "kSameAsRequested",
    "cudnn_conv_algo_search": "HEURISTIC",
    "cudnn_conv_use_max_workspace": "1", "use_tf32": "0",
}


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def _pe(path):
    with path.open("rb") as stream:
        header = stream.read(64)
        if len(header) != 64 or header[:2] != b"MZ":
            raise ValueError("CUDA payload is not a Windows executable")
        offset = struct.unpack_from("<I", header, 60)[0]
        if not 64 <= offset <= min(1024 * 1024, path.stat().st_size - 26):
            raise ValueError("CUDA payload PE offset is invalid")
        stream.seek(offset)
        pe = stream.read(26)
    if pe[:4] != b"PE\0\0" or struct.unpack_from("<H", pe, 4)[0] != 0x8664 or struct.unpack_from("<H", pe, 24)[0] != 0x20B:
        raise ValueError("CUDA runtime requires AMD64 PE32+ libraries")


def validate_cuda_module(path, expected_manifest_sha256):
    """Verify a supplied package, including unqualified private test packages.

    This does not qualify hardware. Production installation additionally requires
    qualified=true after the actual native six-deck acceptance succeeds.
    """
    path = Path(path).absolute()
    if path.is_symlink() or not path.is_dir():
        raise ValueError("CUDA package must be an ordinary directory")
    root = path.resolve(strict=True)
    manifest_path = root / "module.json"
    if manifest_path.is_symlink() or not manifest_path.is_file() or manifest_path.stat().st_size > 16 * 1024:
        raise ValueError("CUDA manifest missing or oversized")
    if not re.fullmatch(r"[0-9a-fA-F]{64}", expected_manifest_sha256) or digest(manifest_path) != expected_manifest_sha256.lower():
        raise ValueError("CUDA manifest checksum mismatch")
    manifest = json.loads(manifest_path.read_bytes())
    pins = {"id": "stems-cuda", "version": 1, "provider": "cuda",
            "onnxruntime_version": "1.22.0", "model_sha256": MODEL_SHA256}
    if any(manifest.get(key) != value for key, value in pins.items()) or type(manifest.get("qualified")) is not bool:
        raise ValueError("CUDA package contract/pins mismatch")
    if manifest.get("provider_options") != OPTIONS:
        raise ValueError("CUDA provider options differ from qualification")
    entries = manifest.get("files")
    if not isinstance(entries, list) or len(entries) != len(FILES):
        raise ValueError("CUDA payload manifest is incomplete")
    seen, total = set(), 0
    for entry in entries:
        name = entry.get("path")
        if name not in FILES or name in seen:
            raise ValueError("Unknown or duplicate CUDA payload path")
        seen.add(name)
        target = root / name
        for ancestor in [target, *target.parents]:
            if ancestor == root:
                break
            if ancestor.is_symlink():
                raise ValueError("Linked CUDA payload is unsupported")
        if not target.resolve(strict=True).is_relative_to(root) or not stat.S_ISREG(target.lstat().st_mode):
            raise ValueError("CUDA payload escapes its package")
        size = entry.get("bytes")
        if type(size) is not int or not 0 < size <= 1024**3 or target.stat().st_size != size:
            raise ValueError("CUDA payload size mismatch")
        total += size
        if total > MAX_BYTES:
            raise ValueError("CUDA package exceeds the fixed runtime budget")
        expected = entry.get("sha256", "")
        if not re.fullmatch(r"[0-9a-fA-F]{64}", expected) or digest(target) != expected.lower():
            raise ValueError("CUDA payload checksum mismatch")
        if target.suffix.lower() == ".dll":
            _pe(target)
    actual = set()
    for item in root.rglob("*"):
        if item.is_symlink():
            raise ValueError("Linked CUDA package member")
        if item.is_file():
            actual.add(item.relative_to(root).as_posix())
    if seen != FILES or actual != FILES | {"module.json"} or manifest.get("payload_bytes") != total:
        raise ValueError("Unexpected CUDA package members or byte accounting")
    return root, manifest
