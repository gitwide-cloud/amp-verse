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
from unittest import mock
import sys

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
    @unittest.skipUnless(os.name == "nt", "Windows console launch flags")
    def test_decoder_and_encoder_hide_child_windows_without_losing_streams(self):
        with mock.patch.object(worker.subprocess,"run",return_value="done") as run:
            self.assertEqual(worker.run_audio(["ffmpeg","decode"],capture_output=True,check=True),"done")
            options=run.call_args.kwargs
            self.assertTrue(options["creationflags"] & subprocess.CREATE_NO_WINDOW)
            self.assertTrue(options["startupinfo"].dwFlags & subprocess.STARTF_USESHOWWINDOW)
            self.assertEqual(options["startupinfo"].wShowWindow,subprocess.SW_HIDE)
            self.assertTrue(options["capture_output"])
            worker.run_audio(["ffmpeg","encode"],input=b"audio",check=True)
            self.assertEqual(run.call_args.kwargs["input"],b"audio")
            self.assertNotIn("stdin",run.call_args.kwargs)
        result=worker.run_audio([sys.executable,"-c","import ctypes; print(ctypes.windll.kernel32.GetConsoleWindow())"],capture_output=True,check=True)
        self.assertEqual(result.stdout.strip(),b"0")

    @unittest.skipUnless(os.name == "nt", "Windows private-runtime compatibility")
    def test_legacy_worker_fix_is_scoped_and_hides_descendants(self):
        with tempfile.TemporaryDirectory() as directory:
            folder=Path(directory)
            shutil.copyfile(REPO / "apps/amp-player/src/stem_sitecustomize.py",folder / "sitecustomize.py")
            env=dict(os.environ,PYTHONPATH=directory,AMP_STEM_FFMPEG="amp-owned-test")
            code="import subprocess,sys; print(subprocess.Popen.__name__); print(subprocess.run([sys.executable,'-c','import ctypes; print(ctypes.windll.kernel32.GetConsoleWindow())'],capture_output=True,check=True).stdout.decode().strip())"
            result=subprocess.run([sys.executable,"-c",code],capture_output=True,check=True,creationflags=subprocess.CREATE_NO_WINDOW,env=env)
            self.assertEqual(result.stdout.decode().splitlines(),["_AmpHiddenPopen","0"])
            env.pop("AMP_STEM_FFMPEG")
            result=subprocess.run([sys.executable,"-c","import subprocess; print(subprocess.Popen.__name__)"],capture_output=True,check=True,creationflags=subprocess.CREATE_NO_WINDOW,env=env)
            self.assertEqual(result.stdout.strip(),b"Popen")

    @unittest.skipUnless(os.environ.get("AMP_STEM_FFMPEG"), "real FFmpeg not selected")
    def test_hidden_audio_decode_export_and_errors_remain_available(self):
        ffmpeg=os.environ["AMP_STEM_FFMPEG"]
        audio=worker.run_audio([ffmpeg,"-v","error","-threads","1","-f","lavfi","-i","sine=frequency=440:duration=0.25","-ac","2","-ar","44100","-f","f32le","pipe:1"],capture_output=True,check=True).stdout
        self.assertEqual(len(audio),11025*2*4)
        with tempfile.TemporaryDirectory() as directory:
            target=Path(directory)/"sample.wav"
            worker.run_audio([ffmpeg,"-v","error","-y","-f","f32le","-ar","44100","-ac","2","-i","pipe:0","-c:a","pcm_f32le",str(target)],input=audio,capture_output=True,check=True)
            self.assertTrue(target.read_bytes().startswith(b"RIFF"))
            self.assertGreater(target.stat().st_size,len(audio))
            with self.assertRaises(subprocess.CalledProcessError) as failure:
                worker.run_audio([ffmpeg,"-v","error","-i",str(Path(directory)/"missing.wav")],capture_output=True,check=True)
            self.assertTrue(failure.exception.stderr)

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
