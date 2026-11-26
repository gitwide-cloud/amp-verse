#!/usr/bin/env python3
"""Fetch only the exact installed, network-disabled Windows decoder for CI smoke.
Does not install or replace AMP Verse tools. The downloaded archive also retains
its original notices; provenance is saved beside the extracted CI executable.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import urllib.request
import zipfile

URL = "https://github.com/gitwide-cloud/mp3-lab/releases/download/v0.2.0-preview.21/mp3-lab-windows.zip"
ARCHIVE_SHA256 = "e31989a38509a091fb370e7649c8527287723b7ada00ecc6012895d1fdf090ac"
EXE_SHA256 = "bda0115411aa4b2e422a923b8723069fc6a60f4d9ec884cc4e941ae2d7d67b28"


def main():
    if os.name != "nt":
        executable = shutil.which("ffplay")
        if not executable:
            raise SystemExit("FFplay is required for the Linux pipe-only test")
    else:
        destination = Path("test-output/receiver-shipped-decoder").resolve()
        destination.mkdir(parents=True, exist_ok=True)
        archive = destination / "mp3-lab-windows.zip"
        digest = hashlib.sha256()
        with urllib.request.urlopen(URL, timeout=30) as response, archive.open("wb") as output:
            size = 0
            while chunk := response.read(1024 * 1024):
                size += len(chunk)
                if size > 400 * 1024 * 1024:
                    raise RuntimeError("Unexpected decoder archive size")
                digest.update(chunk)
                output.write(chunk)
        if digest.hexdigest() != ARCHIVE_SHA256:
            raise RuntimeError("Shipped decoder archive checksum mismatch")
        with zipfile.ZipFile(archive) as bundle:
            data = bundle.read("ffplay.exe")
        if len(data) != 30483456 or hashlib.sha256(data).hexdigest() != EXE_SHA256:
            raise RuntimeError("Shipped decoder executable checksum mismatch")
        executable = str(destination / "ffplay.exe")
        Path(executable).write_bytes(data)
        (destination / "provenance.json").write_text(json.dumps({
            "url": URL, "archive_sha256": ARCHIVE_SHA256,
            "ffplay_sha256": EXE_SHA256,
            "configuration": "FFmpeg 8.1.3 --disable-network; original notices retained in archive",
            "purpose": "CI smoke only; no owner installation or binary replacement",
        }, indent=2), encoding="utf-8")
    if environment := os.environ.get("GITHUB_ENV"):
        with open(environment, "a", encoding="utf-8") as output:
            output.write("AMP_RECEIVER_FFPLAY=" + executable + "\n")
    print("Receiver codec fixture: " + executable)


if __name__ == "__main__":
    main()
