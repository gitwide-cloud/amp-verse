"""CI-only Windows codec provisioning, independent of Chocolatey availability.
Pinned Gyan builder linked by https://ffmpeg.org/download.html#build-windows.
Never changes owner application codecs. Retains the verified archive and its notices.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import struct
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import zipfile

VERSION = "9.1"
ARCHIVE_NAME = "ffmpeg-9.1-essentials_build.zip"
ARCHIVE_SIZE = 119467219
ARCHIVE_SHA256 = "b9bbbb3acece000cbeec994f856649354e52c773807841e78908374b24d658fd"
URLS = (
    "https://github.com/GyanD/codexffmpeg/releases/download/9.1/ffmpeg-9.1-essentials_build.zip",
    "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.1-essentials_build.zip",
)
BINARIES = ("ffmpeg.exe", "ffprobe.exe", "ffplay.exe")
MAX_EXE_BYTES = 256 * 1024 * 1024

class IntegrityError(ValueError):
    pass


def verify_bytes(data, expected_size=ARCHIVE_SIZE, expected_sha256=ARCHIVE_SHA256):
    if len(data) != expected_size or hashlib.sha256(data).hexdigest() != expected_sha256:
        raise IntegrityError("FFmpeg archive size/SHA256 mismatch; refusing this payload")
    return data


def fetch_once(url):
    request = urllib.request.Request(url, headers={"User-Agent": "AMP-Verse-CI-codec-setup/1"})
    deadline = time.monotonic() + 120
    with urllib.request.urlopen(request, timeout=30) as response:
        advertised = response.headers.get("Content-Length")
        if advertised is not None and int(advertised) != ARCHIVE_SIZE:
            raise IntegrityError("Pinned FFmpeg Content-Length mismatch")
        chunks, total = [], 0
        read = getattr(response, "read1", response.read)
        while True:
            if time.monotonic() >= deadline:
                raise TimeoutError("FFmpeg download deadline exceeded")
            chunk = read(min(1024 * 1024, ARCHIVE_SIZE + 1 - total))
            if not chunk:
                break
            total += len(chunk)
            if total > ARCHIVE_SIZE:
                raise IntegrityError("FFmpeg archive exceeds pinned size")
            chunks.append(chunk)
    return verify_bytes(b"".join(chunks))


def download():
    failures = []
    for url in URLS:
        for attempt in range(1, 4):
            try:
                return fetch_once(url), url
            except IntegrityError:
                # A different checksum is not a network outage and must never be bypassed.
                raise
            except (urllib.error.URLError, TimeoutError, OSError) as error:
                failures.append(f"{url} attempt{attempt}: {error}")
                print(f"CI FFmpeg network attempt {attempt}/3 failed: {error}", file=sys.stderr)
                if attempt < 3:
                    time.sleep(attempt * 2)
    raise OSError("Both pinned FFmpeg sources failed after three attempts each. " + " | ".join(failures))


def validate_pe(data):
    if not 128 <= len(data) <= MAX_EXE_BYTES or data[:2] != b"MZ":
        raise IntegrityError("Invalid bounded Windows executable")
    offset = struct.unpack_from("<I", data, 60)[0]
    if not 64 <= offset <= min(1024 * 1024, len(data) - 26):
        raise IntegrityError("Invalid PE header offset")
    if data[offset:offset+4] != b"PE\0\0" or struct.unpack_from("<H", data, offset+4)[0] != 0x8664:
        raise IntegrityError("Expected AMD64 PE executable")
    if struct.unpack_from("<H", data, offset+24)[0] != 0x20B:
        raise IntegrityError("Expected PE32+ executable")


def selected_binaries(data):
    """Exact known archive names only: no extractall, archive-controlled output paths or DLLs."""
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for name in BINARIES:
            member = f"ffmpeg-9.1-essentials_build/bin/{name}"
            matches = [entry for entry in archive.infolist() if entry.filename == member]
            if len(matches) != 1:
                raise IntegrityError(f"Missing/duplicate pinned executable: {name}")
            entry = matches[0]
            if entry.is_dir() or stat.S_ISLNK(entry.external_attr >> 16) or not 128 <= entry.file_size <= MAX_EXE_BYTES:
                raise IntegrityError("Invalid executable archive member")
            content = archive.read(entry)
            validate_pe(content)
            yield name, content


def installed(destination):
    archive = destination / ARCHIVE_NAME
    if archive.is_symlink() or not archive.is_file() or archive.stat().st_size != ARCHIVE_SIZE:
        raise IntegrityError("Existing CI destination is not a verified codec installation")
    data = verify_bytes(archive.read_bytes())
    for name, content in selected_binaries(data):
        path = destination / "bin" / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size != len(content):
            raise IntegrityError("Existing CI executable size mismatch")
        if hashlib.sha256(path.read_bytes()).digest() != hashlib.sha256(content).digest():
            raise IntegrityError("Existing CI executable checksum mismatch")
    return destination / "bin"


def setup(destination):
    destination = Path(destination).absolute()
    if destination.is_symlink():
        raise IntegrityError("CI codec destination must not be a symlink")
    if destination.exists():
        return installed(destination)
    data, url = download()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".amp-ci-ffmpeg-", dir=destination.parent) as temporary:
        stage = Path(temporary) / "payload"
        (stage / "bin").mkdir(parents=True)
        (stage / ARCHIVE_NAME).write_bytes(data)
        files = []
        for name, content in selected_binaries(data):
            (stage / "bin" / name).write_bytes(content)
            files.append({"name": name, "bytes": len(content), "sha256": hashlib.sha256(content).hexdigest()})
        receipt = {"ci_only": True, "version": VERSION, "source_url": url,
                   "archive_bytes": ARCHIVE_SIZE, "archive_sha256": ARCHIVE_SHA256,
                   "publisher_reference": "https://ffmpeg.org/download.html#build-windows",
                   "notices": "Full original builder archive retained with its license/readme/doc notices.",
                   "binaries": files}
        (stage / "PROVENANCE.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        if destination.exists():
            raise IntegrityError("Destination appeared during setup; refusing overwrite")
        stage.rename(destination)
    return destination / "bin"


def export_and_check(binary_directory):
    for name in BINARIES:
        result = subprocess.run([str(binary_directory / name), "-version"], capture_output=True, text=True, timeout=20,
                                creationflags=0x08000000 if os.name == "nt" else 0)
        if result.returncode:
            raise RuntimeError(f"Pinned {name} failed -version: {result.stderr[-500:]}")
        print(result.stdout.splitlines()[0])
    github_path = os.environ.get("GITHUB_PATH")
    if github_path:
        value = str(binary_directory.resolve())
        if "\n" in value or "\r" in value:
            raise ValueError("Invalid CI path")
        with open(github_path, "a", encoding="utf-8", newline="\n") as output:
            output.write(value + "\n")
        print("Verified CI FFmpeg bin exported through GITHUB_PATH")


def self_test():
    # Offline protocol/archive fixtures only; these are not actual executable acceptance.
    import unittest
    class SecurityTests(unittest.TestCase):
        def test_checksum_is_hard_failure(self):
            with self.assertRaises(IntegrityError):
                verify_bytes(b"wrong", 5, hashlib.sha256(b"right").hexdigest())
        def test_exact_selection_ignores_traversal(self):
            pe = bytearray(128)
            pe[:2] = b"MZ"
            struct.pack_into("<I", pe, 60, 64)
            pe[64:68] = b"PE\0\0"
            struct.pack_into("<H", pe, 68, 0x8664)
            struct.pack_into("<H", pe, 88, 0x20B)
            buffer = io.BytesIO()
            with zipfile.ZipFile(buffer, "w") as archive:
                for name in BINARIES:
                    archive.writestr(f"ffmpeg-9.1-essentials_build/bin/{name}", pe)
                archive.writestr("../../owner.exe", pe)
            self.assertEqual([name for name, _ in selected_binaries(buffer.getvalue())], list(BINARIES))
        def test_non_pe_is_rejected(self):
            with self.assertRaises(IntegrityError):
                validate_pe(b"not an executable" * 20)
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(SecurityTests))
    return result.wasSuccessful()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dest", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        raise SystemExit(0 if self_test() else 1)
    if args.dest is None:
        temporary = os.environ.get("RUNNER_TEMP")
        if not temporary:
            parser.error("RUNNER_TEMP or an explicit --dest is required")
        args.dest = Path(temporary) / "amp-ci-ffmpeg"
    try:
        binaries = setup(args.dest)
        export_and_check(binaries)
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        parser.exit(1, f"Verified CI FFmpeg setup failed: {error}\n")

if __name__ == "__main__":
    main()
