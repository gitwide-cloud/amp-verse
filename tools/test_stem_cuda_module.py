"""Offline integrity tests; fixtures contain no executable code or song audio."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest

from stem_cuda_module import FILES, MODEL_SHA256, OPTIONS, validate_cuda_module


class PackageIntegrity(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="amp-cuda-contract-")
        self.root = Path(self.temp.name).resolve()
        pe = bytearray(128)
        pe[:2] = b"MZ"
        struct.pack_into("<I", pe, 60, 64)
        pe[64:68] = b"PE\0\0"
        struct.pack_into("<H", pe, 68, 0x8664)
        struct.pack_into("<H", pe, 88, 0x20B)
        entries = []
        for name in sorted(FILES):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            data = bytes(pe) if name.endswith(".dll") else b"test notice"
            path.write_bytes(data)
            entries.append({"path": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
        self.manifest = {"id": "stems-cuda", "version": 1, "provider": "cuda",
                         "onnxruntime_version": "1.22.0", "model_sha256": MODEL_SHA256,
                         "qualified": False, "provider_options": OPTIONS.copy(),
                         "files": entries, "payload_bytes": sum(e["bytes"] for e in entries)}
        self.pin = self.write_manifest()

    def tearDown(self):
        # The only recursive cleanup target is the exact fresh TemporaryDirectory.
        assert self.root.parent == Path(tempfile.gettempdir()).resolve()
        assert self.root.name.startswith("amp-cuda-contract-")
        self.temp.cleanup()

    def write_manifest(self):
        data = json.dumps(self.manifest).encode()
        (self.root / "module.json").write_bytes(data)
        return hashlib.sha256(data).hexdigest()

    def test_explicit_private_qualification_package_is_verified_but_not_marked_ready(self):
        root, result = validate_cuda_module(self.root, self.pin)
        self.assertEqual(root, self.root)
        self.assertFalse(result["qualified"])

    def test_manifest_pin_is_required(self):
        with self.assertRaises(ValueError):
            validate_cuda_module(self.root, "0" * 64)

    def test_changed_binary_is_rejected(self):
        (self.root / "onnxruntime.dll").write_bytes(b"corrupt")
        with self.assertRaises(ValueError):
            validate_cuda_module(self.root, self.pin)

    def test_unmanifested_payload_is_rejected(self):
        (self.root / "extra.dll").write_bytes(b"extra")
        with self.assertRaises(ValueError):
            validate_cuda_module(self.root, self.pin)

    def test_traversal_is_rejected_even_with_valid_manifest_pin(self):
        self.manifest["files"][0]["path"] = "../outside.dll"
        with self.assertRaises(ValueError):
            validate_cuda_module(self.root, self.write_manifest())

    def test_unqualified_provider_options_cannot_reuse_contract(self):
        self.manifest["provider_options"]["use_tf32"] = "1"
        with self.assertRaises(ValueError):
            validate_cuda_module(self.root, self.write_manifest())


if __name__ == "__main__":
    unittest.main()
