"""Local integration checks. Set AMP_STEM_PYTHON and AMP_ENGINE_EXE for real workers."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import threading
from contextlib import contextmanager
import unittest

REPO = Path(__file__).resolve().parents[1]
WORKER = REPO / "apps/amp-player/src/stem_worker.py"
spec = importlib.util.spec_from_file_location("stem_worker", WORKER)
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


@contextmanager
def playback_marker(marker, mode):
    stopped = threading.Event()
    def renew():
        while not stopped.is_set():
            marker.write_text(str(int(time.time() * 1000) + 8000) + " " + mode)
            stopped.wait(0.1)
    thread = threading.Thread(target=renew)
    thread.start()
    try:
        yield
    finally:
        stopped.set()
        thread.join()


class PreparationTests(unittest.TestCase):
    def test_expiring_pause_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "pause"
            for content, expected in [("1200", True), ("900", False), ("bad", False), ("9999999", False)]:
                marker.write_text(content)
                self.assertEqual(worker.pause_active(marker, 1000), expected)
            marker.write_text("1200 slow")
            self.assertEqual(worker.activity_at(marker,1000),"slow")
            self.assertFalse(worker.pause_active(marker,1000))
            marker.write_text("1200 pause")
            self.assertTrue(worker.pause_active(marker,1000))

    @unittest.skipUnless(os.environ.get("AMP_ENGINE_EXE"), "real analysis engine not selected")
    def test_real_analysis_switches_from_pause_to_slow_and_reuses_completed_work(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "song.wav"
            shutil.copyfile(os.environ["AMP_WAVEFORM_FIXTURE"], source)
            before = hashlib.sha256(source.read_bytes()).hexdigest()
            marker = root / "pause"
            marker.write_text(str(int(time.time() * 1000) + 8000))
            env = dict(os.environ, AMP_PREPARATION_PAUSE_FILE=str(marker), AMP_PREPARATION_CACHE_DIR=str(root / "cache"), AMP_DEVICE_CONFIG_DIR=str(root / "device"))
            request = json.dumps({"protocol": 1, "root": str(root), "relative": "song.wav", "draft": None}).encode()
            process = subprocess.Popen([os.environ["AMP_ENGINE_EXE"]], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
            try:
                process.stdin.write(request)
                process.stdin.close()
                time.sleep(1)
                self.assertIsNone(process.poll(), "analysis must wait for playback")
                with playback_marker(marker, "slow"):
                    process.wait(timeout=40)
                    self.assertEqual(worker.activity_at(marker), "slow", "analysis must complete while playback is still active")
                self.assertEqual(json.loads(process.stdout.read())["status"], "prepared")
                repeated = subprocess.run([os.environ["AMP_ENGINE_EXE"]], input=request, capture_output=True, env=env, timeout=20, check=True)
                self.assertEqual(json.loads(repeated.stdout)["status"], "skipped")
                self.assertFalse(list((root / "cache").glob("analysis-*")))
                self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), before)
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait()
                process.stdout.close()
                process.stderr.close()

    @unittest.skipUnless(os.environ.get("AMP_STEM_PYTHON"), "real stem runtime not selected")
    def test_real_gpu_stems_switch_from_pause_to_slow_and_finish_during_playback(self):
        python = os.environ["AMP_STEM_PYTHON"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "song.wav"
            # A longer local fixture exercises checkpoints between model segments.
            subprocess.run([os.environ["AMP_STEM_FFMPEG"], "-v", "error", "-y", "-stream_loop", "-1", "-i", os.environ["AMP_WAVEFORM_FIXTURE"], "-t", "35", str(source)], check=True)
            before = hashlib.sha256(source.read_bytes()).hexdigest()
            marker = root / "pause"
            marker.write_text("0")
            env = dict(os.environ, AMP_PREPARATION_PAUSE_FILE=str(marker), AMP_STEM_THREADS="2")
            log = root / "worker.log"
            with log.open("w") as output:
                process = subprocess.Popen([python, str(WORKER), str(source), str(root / "result")], stdout=output, stderr=output, env=env)
                try:
                    deadline = time.monotonic() + 120
                    while "Separating on" not in log.read_text(errors="replace"):
                        self.assertIsNone(process.poll(), log.read_text(errors="replace"))
                        self.assertLess(time.monotonic(), deadline)
                        time.sleep(0.02)
                    marker.write_text(str(int(time.time() * 1000) + 9000))
                    while "Paused for playback" not in log.read_text(errors="replace"):
                        self.assertIsNone(process.poll(), log.read_text(errors="replace"))
                        self.assertLess(time.monotonic(), deadline)
                        time.sleep(0.02)
                    time.sleep(1)
                    self.assertIsNone(process.poll())
                    self.assertFalse((root / "result/audio.json").exists())
                    with playback_marker(marker, "slow"):
                        self.assertEqual(process.wait(timeout=120), 0, log.read_text(errors="replace"))
                        self.assertEqual(worker.activity_at(marker), "slow")
                    self.assertIn("Working slowly during playback", log.read_text(errors="replace"))
                    manifest = json.loads((root / "result/audio.json").read_text())
                    self.assertEqual(manifest["device"], "cuda")
                    self.assertEqual(set(manifest["files"]), {"vocals", "instrumental", "drums"})
                    for name, record in manifest["files"].items():
                        self.assertEqual(hashlib.sha256((root / "result" / (name + ".wav")).read_bytes()).hexdigest(), record["sha256"])
                    self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), before)
                finally:
                    if process.poll() is None:
                        process.kill()
                    process.wait()


if __name__ == "__main__":
    unittest.main(verbosity=2)
