"""Real encoder/network smoke test, standard library only; requires FFmpeg on PATH."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def main():
    binary = str(Path(sys.argv[1]).resolve())
    with tempfile.TemporaryDirectory(prefix="amp-cast-") as tmp:
        root = Path(tmp)
        for name, freq in [("a.wav", 440), ("b.wav", 660)]:
            subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", f"sine=frequency={freq}:duration=2", str(root / name)], check=True)
        (root / "a.wav.amp.json").write_text(json.dumps({"cue_in": 0.2, "cue_out": 1.2}))
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        env = {**os.environ, "AMP_CAST_BIND": f"127.0.0.1:{port}", "AMP_CAST_PASSWORD": "test-only"}
        process = subprocess.Popen([binary, tmp], env=env)
        def request(path, method="GET", password=None):
            headers = {"x-amp-password": password} if password else {}
            return urllib.request.urlopen(urllib.request.Request(base+path, method=method, headers=headers), timeout=8)
        def status():
            with request("/api/status") as response:
                return json.load(response)
        try:
            for _ in range(80):
                try:
                    if status()["tracks"] == 2:
                        break
                except (OSError, urllib.error.URLError):
                    time.sleep(0.1)
            else:
                raise AssertionError("Server failed to start")
            with request("/api/playlist") as response:
                playlist = json.load(response)["tracks"]
            assert [track["title"] for track in playlist] == ["a", "b"]
            assert [track["index"] for track in playlist] == [0, 1]
            assert playlist[0]["cue_in"] == 0.2 and playlist[0]["cue_out"] == 1.2
            assert all("path" not in track for track in playlist)
            try:
                request("/api/start", "POST")
                raise AssertionError("Missing password accepted")
            except urllib.error.HTTPError as error:
                assert error.code == 401
            with request("/api/start", "POST", "test-only") as response:
                assert response.status == 204
            with request("/listen.mp3") as listener, request("/listen.mp3") as second:
                data = listener.read(12000)
                assert len(data) == 12000
                assert len(second.read(4096)) == 4096
                assert status()["listeners"] >= 2
                assert status()["current_index"] in (0, 1)
                # Decode captured bytes to prove this is audio, not just a successful HTTP response.
                captured = root / "capture.mp3"
                captured.write_bytes(data)
                subprocess.run(["ffmpeg", "-v", "error", "-i", str(captured), "-f", "null", "-"], check=True)
            observed = set()
            for _ in range(60):
                state = status()
                assert state["running"], state
                observed.add(state["now_playing"])
                if {"a", "b"}.issubset(observed):
                    break
                time.sleep(0.1)
            assert {"a", "b"}.issubset(observed), observed
            with request("/api/stop", "POST", "test-only") as response:
                assert response.status == 204
            assert not status()["running"]
            assert status()["current_index"] is None
            try:
                request("/listen.mp3")
                raise AssertionError("Stopped station still accepts listeners")
            except urllib.error.HTTPError as error:
                assert error.code == 503
            print("PASS: password, start/stop, two shared listeners, valid MP3 decode, Auto DJ track transitions")
        finally:
            process.terminate()
            process.wait(timeout=10)


if __name__ == "__main__":
    main()
