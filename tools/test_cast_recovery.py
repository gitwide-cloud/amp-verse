"""Opt-in Pixel/ExoPlayer test against a real AMP server with one failing proxy link.
Requires AMP_ADB, AMP_CAST_EXE, AMP_FFMPEG and the matching installed test APK.
Uses loopback/ADB forwarding; this is not a physical Ethernet failover test.
"""
import os
from pathlib import Path
import subprocess
import time
import json
import urllib.request
import urllib.error
import threading
import http.server
import socket
import shlex

root = Path(os.environ.get("AMP_CAST_TEST_DIR", "test-output/cast-recovery/fixture")).resolve()
music = root / "music"
music.mkdir(parents=True, exist_ok=True)
adb = os.environ["AMP_ADB"]
subprocess.run([os.environ["AMP_FFMPEG"], "-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=18", "-ac", "2", str(music / "recovery.wav")], check=True)
with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    port = listener.getsockname()[1]
backend = "http://127.0.0.1:" + str(port)
env = dict(os.environ, AMP_CAST_BIND="127.0.0.1:" + str(port), AMP_CAST_CONFIG_DIR=str(root / "config"), AMP_DEVICE_CONFIG_DIR=str(root / "device"), AMP_DOCUMENTS_DIR=str(root / "documents"), AMP_ANALYSIS_ENABLED="false")
log = (root / "server.log").open("w")
server = subprocess.Popen([os.environ["AMP_CAST_EXE"], str(music)], env=env, stdout=log, stderr=log, creationflags=0x08004000 if os.name == "nt" else 0)
proxies = []
counts = {"drops": 0, "backup": 0}
started = [None]

class Proxy(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_GET(self):
        if self.path.startswith("/api/audio/manifest") and started[0] is None:
            started[0] = time.monotonic()
        if self.path.startswith("/api/audio/chunks/"):
            if self.server is proxies[1]:
                counts["backup"] += 1
            if self.server is proxies[0] and started[0] and time.monotonic() - started[0] > 8:
                counts["drops"] += 1
                self.close_connection = True
                return
        try:
            headers = {key: value for key, value in self.headers.items() if key.lower().startswith("x-amp-")}
            request = urllib.request.Request(backend + self.path, headers=headers)
            try:
                response = urllib.request.urlopen(request, timeout=4)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                data = response.read()
                status = response.status
                response_headers = dict(response.headers)
            if self.path.startswith("/api/audio/manifest") and status == 200:
                value = json.loads(data)
                value["paths"] = ["http://127.0.0.1:" + str(proxy.server_port) for proxy in proxies]
                data = json.dumps(value).encode()
            self.send_response(status)
            for key, value in response_headers.items():
                if key.lower() not in ["content-length", "connection", "transfer-encoding"]:
                    self.send_header(key, value)
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(data)
        except (OSError, TimeoutError):
            self.close_connection = True

try:
    deadline = time.monotonic() + 15
    while True:
        try:
            with urllib.request.urlopen(backend + "/api/status", timeout=2) as response:
                status = json.load(response)
            break
        except OSError:
            assert time.monotonic() < deadline, "Fixture server did not start"
            time.sleep(0.1)
    with urllib.request.urlopen(urllib.request.Request(backend + "/api/start", data=b""), timeout=3) as response:
        assert 200 <= response.status < 300
    time.sleep(7)
    for _ in range(2):
        proxy = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Proxy)
        proxy.daemon_threads = True
        proxies.append(proxy)
    for proxy in proxies:
        threading.Thread(target=proxy.serve_forever, daemon=True).start()
        subprocess.run([adb, "reverse", "tcp:" + str(proxy.server_port), "tcp:" + str(proxy.server_port)], check=True)
    url = "http://127.0.0.1:" + str(proxies[0].server_port) + "/amp-protected.mp3?server=" + status["server_id"] + "&station=" + status["station_id"]
    result = subprocess.run([adb, "shell", "am", "instrument", "-w", "-e", "class", "cloud.gitwide.ampverse.ProtectedCastDeviceTest#real_mp3_plays_through_service_without_rebuffering_during_path_failure", "-e", "protectedUrl", shlex.quote(url), "cloud.gitwide.ampverse.test/androidx.test.runner.AndroidJUnitRunner"], capture_output=True, text=True, timeout=60)
    print(result.stdout)
    print(result.stderr)
    assert "OK (1 test)" in result.stdout, result.stdout
    assert counts["drops"] > 0 and counts["backup"] > 0, counts
    (root / "verified.txt").write_text("Real Pixel playback: primary deliberately dropped after startup; secondary supplied chunks; zero rebuffer events.\n" + str(counts) + "\n")
    print("Fault injection:", counts)
finally:
    for proxy in proxies:
        proxy.shutdown()
        proxy.server_close()
        subprocess.run([adb, "reverse", "--remove", "tcp:" + str(proxy.server_port)], capture_output=True)
    server.terminate()
    server.wait(timeout=10)
    log.close()
