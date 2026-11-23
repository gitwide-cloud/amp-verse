"""Native live-stem acceptance. Generated/user-read-only PCM stays in RAM; no stem/audio files.
--unit runs labeled offline packager fixtures. --module runs the actual native SCNet model.
Reconstruction measures wire/mix correctness, never subjective vocal-separation quality.
"""
import argparse
import array
import hashlib
import io
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock
import zipfile
import setup_live_stems as package

FRAMES = 485100

def validate_module(root):
    root = Path(root).resolve()
    manifest_path = root / "module.json"
    if not manifest_path.is_file() or manifest_path.stat().st_size > 65536:
        raise ValueError("Bounded module manifest required")
    manifest = json.loads(manifest_path.read_bytes())
    if manifest.get("schema") != 1 or manifest.get("model_sha256") != package.MODEL.sha256 or manifest.get("provider") != "cpu":
        raise ValueError("Unexpected native module identity")
    all_entries = list(root.rglob("*"))
    if any(p.is_symlink() for p in all_entries):
        raise ValueError("Module symlinks are not permitted")
    actual = {p.relative_to(root).as_posix() for p in all_entries if p.is_file()}
    expected = {"module.json"}
    for entry in manifest["payload_files"]:
        relative = Path(entry["path"])
        if relative.is_absolute() or ".." in relative.parts or entry["path"] in expected:
            raise ValueError("Unsafe/duplicate module path")
        expected.add(entry["path"])
        if type(entry["bytes"]) is not int or not 0 < entry["bytes"] <= package.MAX_MODULE_BYTES:
            raise ValueError("Module payload byte limit exceeded")
        path = root / relative
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root):
            raise ValueError("Invalid module payload")
        if path.stat().st_size != entry["bytes"] or hashlib.sha256(path.read_bytes()).hexdigest() != entry["sha256"]:
            raise ValueError("Module payload size/hash mismatch")
    total = sum((root / name).stat().st_size for name in actual)
    if actual != expected or len(actual) != manifest["file_count"] or total != manifest["total_bytes"]:
        raise ValueError("Module accounting mismatch")
    if not package.MIN_MODULE_BYTES <= total <= package.MAX_MODULE_BYTES or manifest.get("persistent_audio_bytes") != 0 or manifest.get("python_runtime_included") is not False:
        raise ValueError("Module exceeds native-only build safety bound")
    for key in ("worker", "runtime", "model"):
        if manifest["relative_paths"][key] not in expected:
            raise ValueError("Module entrypoint is not verified")
    return root, manifest


def kill(process):
    if process.poll() is None:
        try:
            if os.name == "nt":
                process.kill()  # Standalone native worker never launches subprocesses.
            else:
                os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def command(root, manifest):
    paths = manifest["relative_paths"]
    env = os.environ.copy()
    env["ORT_DYLIB_PATH"] = str(root / paths["runtime"])
    args = [str(root / paths["worker"]), "--model", str(root / paths["model"]),
            "--provider", "cpu", "--requests", "0", "--memory-mib", "3072"]
    return args, env


def spawn(root, manifest):
    args, env = command(root, manifest)
    return subprocess.Popen(args, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            start_new_session=os.name != "nt", creationflags=0x08000000 if os.name == "nt" else 0)


def read_exact(stream, size):
    chunks = bytearray()
    while len(chunks) < size:
        block = stream.read(size - len(chunks))
        if not block:
            raise ValueError("Native worker exited with incomplete PCM")
        chunks.extend(block)
    return chunks


def send(process, audio, identity):
    header = dict(version=1, request_id=identity, frames=len(audio)//2, sample_rate=44100, channels=2)
    process.stdin.write(json.dumps(header).encode() + b"\n")
    process.stdin.write(audio.tobytes())
    process.stdin.flush()


def exchange(process, audio, identity):
    timer = threading.Timer(45, kill, [process])
    timer.daemon = True
    timer.start()
    begin = time.monotonic()
    try:
        send(process, audio, identity)
        line = process.stdout.readline(1025)
        if not line or len(line) > 1024 or not line.endswith(b"\n"):
            raise ValueError("Invalid bounded native response header")
        header = json.loads(line)
        if (header.get("request_id") != identity or header.get("frames") != len(audio)//2
                or header.get("version") != 1 or header.get("sample_rate") != 44100 or header.get("channels") != 2
                or header.get("provider") != "CPUExecutionProvider" or header.get("parts") != ["vocals", "drums", "bass", "other"]
                or header.get("hard_model_memory_limit_enforced") is not True or header.get("memory_limit_mib") != 3072
                or header.get("persistent_audio_bytes") != 0):
            raise ValueError("Native response identity/provider/shape/memory mismatch")
        parts = []
        for _ in range(4):
            part = array.array("f")
            part.frombytes(read_exact(process.stdout, len(audio)*4))
            if sys.byteorder != "little":
                part.byteswap()
            if len(part) != len(audio) or not all(math.isfinite(v) for v in part):
                raise ValueError("Invalid real model PCM")
            parts.append(part)
        elapsed = time.monotonic() - begin
        max_error = max(abs(sum(v)-original) for original, v in zip(audio, zip(*parts)))
        if max_error >= 1e-5:
            raise ValueError(f"Original mix reconstruction failed: {max_error}")
        rms = [math.sqrt(sum(v*v for v in part)/len(part)) for part in parts]
        if not any(value > 1e-9 for value in rms[:3]):
            raise ValueError("Real model produced no finite source estimates")
        return {"wall_seconds": elapsed, "header": header, "max_reconstruction_error": max_error,
                "part_rms_vocals_drums_bass_other": rms}
    finally:
        timer.cancel()


def fixture():
    audio = array.array("f")
    for i in range(FRAMES):
        t = i / 44100
        envelope = .65 + .35*math.sin(2*math.pi*3*t)
        audio.extend([envelope*(.22*math.sin(2*math.pi*220*t)+.13*math.sin(2*math.pi*660*t))+.04*math.sin(2*math.pi*61*t),
                      envelope*(.19*math.sin(2*math.pi*220*t+.2)+.12*math.sin(2*math.pi*880*t))+.04*math.sin(2*math.pi*61*t)])
    return audio


def source_pcm(source):
    source = Path(source)
    if not source.is_file():
        raise ValueError("Optional --source must name a local read-only audio file")
    before = (source.stat().st_size, source.stat().st_mtime_ns)
    result = subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-protocol_whitelist", "file,pipe", "-i", str(source),
                             "-t", "11", "-vn", "-ac", "2", "-ar", "44100", "-f", "f32le", "pipe:1"],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    if result.returncode or not 0 < len(result.stdout) <= FRAMES*8 or len(result.stdout) % 8:
        raise ValueError("Read-only fixture FFmpeg failed: " + result.stderr[-400:].decode(errors="replace"))
    if before != (source.stat().st_size, source.stat().st_mtime_ns):
        raise ValueError("Source changed during read-only acceptance")
    audio = array.array("f")
    audio.frombytes(result.stdout)
    if sys.byteorder != "little":
        audio.byteswap()
    return audio


def startup_and_abort_checks(root, manifest, audio):
    args, env = command(root, manifest)
    for payload, success in [(b"", True), (b"x"*1025, False)]:
        result = subprocess.run(args, input=payload, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10,
                                creationflags=0x08000000 if os.name == "nt" else 0)
        if (result.returncode == 0) != success or result.stdout:
            raise ValueError("Actual native EOF/malformed-header handling failed")
    process = spawn(root, manifest)
    try:
        send(process, audio, "abort-fixture")
        time.sleep(.35)
        if process.poll() is not None:
            raise ValueError("Native worker exited before the active abort check")
        kill(process)
        process.wait(timeout=3)
        if process.poll() is None:
            raise ValueError("Native worker survived abort/reap")
    finally:
        kill(process)
        process.communicate(timeout=3)


def throughput(results, budget):
    if len(results) < 3:
        raise ValueError("At least one cold and two warm windows are required")
    times = [entry["wall_seconds"] for entry in results]
    warm_pair = max(a+b for a, b in zip(times[1:], times[2:]))
    return {"cold_first_window_seconds": times[0],
            "worst_including_cold_pair_seconds": max(a+b for a, b in zip(times, times[1:])),
            "worst_serial_pair_seconds": warm_pair,
            "two_deck_stride_seconds": budget, "two_deck_throughput_pass": warm_pair <= budget}


def acceptance(module, requests=4, source=None, max_pair_seconds=10.5):
    if not 3 <= requests <= 16:
        raise ValueError("Acceptance requests must be3..16 (one cold plus two warm minimum)")
    root, manifest = validate_module(module)
    audio = fixture()
    process = spawn(root, manifest)
    results = []
    try:
        for index in range(requests):
            results.append(exchange(process, audio, f"fixture-{index}"))
        music = exchange(process, source_pcm(source), "readonly-music") if source else None
        process.stdin.close()
        process.wait(timeout=10)
        errors = process.stderr.read(8192).replace(b"\0", b"").decode(errors="replace")
        if process.returncode or process.stdout.read(1):
            raise ValueError("Persistent native worker did not exit cleanly at EOF: " + errors[-1000:])
    except Exception:
        kill(process)
        process.wait(timeout=3)
        errors = process.stderr.read(8192).replace(b"\0", b"").decode(errors="replace")
        if errors:
            print(errors[-1000:], file=sys.stderr)
        raise
    finally:
        kill(process)
        for stream in (process.stdin, process.stdout, process.stderr):
            if not stream.closed:
                stream.close()
    startup_and_abort_checks(root, manifest, audio)
    report = {"real_native_model": True, "fixture": "generated stereo harmonics; not listening-quality proof",
              "module_bytes": manifest["total_bytes"], "persistent_audio_bytes": 0,
              "request_results": results, "readonly_music_result": music,
              "active_abort_reaped": True, "clean_eof_and_malformed_pass": True}
    report.update(throughput(results, max_pair_seconds))
    return report


class OfflinePackagingTests(unittest.TestCase):
    """FAKE module fixtures only; never presented as model-inference acceptance."""
    def test_cold_priming_is_reported_separately_from_sustainable_pair(self):
        report = throughput([{"wall_seconds": 9}, {"wall_seconds": 4}, {"wall_seconds": 4.5}], 10.5)
        self.assertEqual(report["cold_first_window_seconds"], 9)
        self.assertEqual(report["worst_including_cold_pair_seconds"], 13)
        self.assertEqual(report["worst_serial_pair_seconds"], 8.5)
        self.assertTrue(report["two_deck_throughput_pass"])
        self.assertFalse(throughput([{"wall_seconds": 1}, {"wall_seconds": 6}, {"wall_seconds": 6}], 10.5)["two_deck_throughput_pass"])
        with self.assertRaises(ValueError):
            throughput([{"wall_seconds": 1}, {"wall_seconds": 1}], 10.5)

    def test_pins_reject_wrong_size_and_hash(self):
        asset = package.Asset("https://fixture.invalid/a", 2, hashlib.sha256(b"ok").hexdigest())
        self.assertEqual(package.checked(b"ok", asset), b"ok")
        for value in (b"no", b"too big"):
            with self.assertRaises(ValueError):
                package.checked(value, asset)

    def test_archive_selection_cannot_write_python_or_traversal(self):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            for name in ("onnxruntime/capi/libonnxruntime.so.1.22.0", "onnxruntime/LICENSE", "onnxruntime/ThirdPartyNotices.txt", "../../escape.py", "onnxruntime/__init__.py"):
                archive.writestr(name, b"fixture")
        files = package.runtime_files(buffer.getvalue(), "onnxruntime/capi/libonnxruntime.so.1.22.0", "libonnxruntime.so")
        self.assertEqual(set(files), {"libonnxruntime.so", "LICENSES/ONNXRuntime-LICENSE.txt", "LICENSES/ONNXRuntime-ThirdPartyNotices.txt"})

    def test_module_counts_hashes_no_overwrite_and_tamper(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "crates/amp-stem-worker").mkdir(parents=True)
            (root / "crates/amp-stem-worker/NOTICES.md").write_text("FAKE fixture notice")
            (root / "LICENSE").write_text("FAKE fixture project license")
            worker = root / "fixture-worker"
            worker.write_bytes(b"\x7fELFfake-not-a-model")
            buffer = io.BytesIO()
            with zipfile.ZipFile(buffer, "w") as archive:
                for name in ("onnxruntime/capi/fake.so", "onnxruntime/LICENSE", "onnxruntime/ThirdPartyNotices.txt"):
                    archive.writestr(name, b"FAKE fixture native/license")
            blobs = {"model": b"FAKE model", "license": b"FAKE license", "runtime": buffer.getvalue()}
            pins = {name: package.Asset("https://fixture.invalid/"+name, len(data), hashlib.sha256(data).hexdigest()) for name, data in blobs.items()}
            with mock.patch.multiple(package, ROOT=root, MIN_MODULE_BYTES=0, MODEL=pins["model"], MODEL_LICENSE=pins["license"],
                                     RUNTIMES={"linux-x64": (pins["runtime"], "onnxruntime/capi/fake.so", "libonnxruntime.so", "amp-stem-worker")}):
                fetch = lambda pin: blobs[pin.url.rsplit("/", 1)[1]]
                output = root / "module"
                manifest = package.package(worker, output, "linux-x64", fetch=fetch)
                validate_module(output)
                self.assertEqual(manifest["total_bytes"], sum(p.stat().st_size for p in output.rglob("*") if p.is_file()))
                self.assertFalse(any(p.suffix == ".py" for p in output.rglob("*")))
                with self.assertRaises(ValueError):
                    package.package(worker, output, "linux-x64", fetch=fetch)
                (output / "scnet.int8.onnx").write_bytes(b"tamper")
                with self.assertRaises(ValueError):
                    validate_module(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--unit", action="store_true", help="Offline FAKE packaging fixtures; no model claim")
    parser.add_argument("--module")
    parser.add_argument("--requests", type=int, default=4)
    parser.add_argument("--source", help="Optional local audio: FFmpeg stdout to RAM only")
    parser.add_argument("--max-pair-seconds", type=float, default=10.5)
    args = parser.parse_args()
    if args.unit:
        unittest.main(argv=[sys.argv[0]], verbosity=2)
        return
    if not args.module or not 0 < args.max_pair_seconds <= 30:
        parser.error("--module is required; max-pair-seconds must be0..30")
    try:
        report = acceptance(args.module, args.requests, args.source, args.max_pair_seconds)
        print(json.dumps(report, indent=2, sort_keys=True))
        if not report["two_deck_throughput_pass"]:
            parser.exit(1, "Actual CPU throughput did not meet the two-deck stride; do not claim two-deck readiness.\n")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Native live-stem acceptance failed: {error}\n")

if __name__ == "__main__":
    main()
