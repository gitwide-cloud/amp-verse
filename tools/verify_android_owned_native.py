#!/usr/bin/env python3
"""Run an emulator instrumentation command against an isolated real AMP server.
Generated audio stays in pipes/memory. Never point this at a production server.
"""
import argparse
from array import array
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import urllib.request

BASE = "http://127.0.0.1:28987"
PASSWORD = "android-owned-fixture"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server-binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("An instrumentation command is required")
    args.output.mkdir(parents=True, exist_ok=True)
    stop = threading.Event()
    encoded = bytearray()
    errors = []

    def request(path):
        return urllib.request.Request(BASE + path, headers={"x-amp-password": PASSWORD})

    def receiver():
        try:
            while not stop.is_set():
                try:
                    with urllib.request.urlopen(request("/api/status"), timeout=1) as response:
                        ready = json.load(response).get("running", False)
                except OSError:
                    ready = False
                if ready:
                    break
                stop.wait(0.025)
            if stop.is_set():
                return
            with urllib.request.urlopen(request("/listen.mp3"), timeout=3) as response:
                while not stop.is_set():
                    chunk = response.read(4096)
                    if not chunk:
                        break
                    if len(encoded) + len(chunk) > 512 * 1024:
                        raise RuntimeError("Generated live receiver exceeded its bounded memory budget")
                    encoded.extend(chunk)
        except Exception as error:
            errors.append(str(error))

    with tempfile.TemporaryDirectory(prefix="amp-native-fixture-", dir=os.environ.get("RUNNER_TEMP")) as folder:
        root = Path(folder)
        for name in ("music", "config", "device"):
            (root / name).mkdir()
        env = os.environ.copy()
        env.update(AMP_CAST_BIND="127.0.0.1:28987", AMP_CAST_PASSWORD=PASSWORD,
                   AMP_CAST_CONFIG_DIR=str(root / "config"), AMP_DEVICE_CONFIG_DIR=str(root / "device"),
                   AMP_ANALYSIS_ENABLED="false", AMP_CAST_DISCOVERY_PORT="0")
        with (args.output / "server.log").open("w") as log:
            server = subprocess.Popen([str(args.server_binary.resolve()), str(root / "music")],
                                      env=env, stdout=log, stderr=log)
            listener = threading.Thread(target=receiver, daemon=True)
            try:
                deadline = time.monotonic() + 10
                while True:
                    try:
                        with urllib.request.urlopen(request("/api/status"), timeout=1) as response:
                            identity = json.load(response)
                        break
                    except OSError:
                        if server.poll() is not None or time.monotonic() > deadline:
                            raise RuntimeError("Isolated server did not start")
                        time.sleep(0.05)
                listener.start()
                test = subprocess.run(command, timeout=600)
                listener.join(5)
                if test.returncode:
                    raise RuntimeError("Android native instrumentation failed")
                if listener.is_alive() or errors:
                    raise RuntimeError("Receiver did not finish cleanly: " + repr(errors))
                decoded = subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "mp3", "-i", "pipe:0",
                                          "-f", "s16le", "-ar", "48000", "-ac", "2", "pipe:1"],
                                         input=bytes(encoded), capture_output=True, timeout=10, check=True)
                samples = array("h")
                samples.frombytes(decoded.stdout)
                seconds = len(samples) / 96000
                rms = math.sqrt(sum(int(value) ** 2 for value in samples) / max(1, len(samples)))
                if not (4 <= seconds <= 12 and rms > 200):
                    raise RuntimeError(f"Invalid decoded generated audio: seconds={seconds}, RMS={rms}")
                receipt = {"passed": True, "server_binary_sha256": hashlib.sha256(args.server_binary.read_bytes()).hexdigest(),
                           "encoded_bytes": len(encoded), "decoded_seconds": seconds, "decoded_rms": rms,
                           "scope": "Actual Android FFmpegKit FIFO + owned PCM/virtual AudioTrack + real isolated server + decoded receiver; not physical speaker/LAN latency",
                           "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()}
                (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
                print(json.dumps(receipt, indent=2))
            finally:
                stop.set()
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()
                if listener.ident:
                    listener.join(5)


if __name__ == "__main__":
    main()
