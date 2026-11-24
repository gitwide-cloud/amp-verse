#!/usr/bin/env python3
"""Qualify owned PCM and live-session logic against pinned Android/Media3 jars.

This runs real Media3 DSP with a fake hardware clock. It does not qualify a
handset, AudioTrack driver, Android encoder, or speaker latency. Dependencies
are supplied explicitly; this script never downloads or installs software.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--java-home", type=Path, required=True)
    parser.add_argument("--api-jars", type=Path, required=True)
    parser.add_argument("--test-jars", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--full-cast", action="store_true", help="Also compile Full encoder against its supplied pinned FFmpegKit API jar")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    source = root / "android/app/src"
    classes = args.output / "classes"
    classes.mkdir(parents=True, exist_ok=True)
    jars = sorted(set(args.api_jars.glob("*.jar")) | set(args.test_jars.glob("*.jar")))
    classpath = os.pathsep.join([str(classes)] + [str(path) for path in jars])
    production = ["OwnedPcmMixer", "MasterPcmQueue", "OwnedPcmOutput", "OwnedAudioOutput",
                  "OwnedMediaAudioSink", "OwnedInputPcm", "AudioDsp", "PhoneLiveSession",
                  "PhoneLiveHttpTransport", "OwnedSamplerVoices", "OwnedSamplerAudio", "PhoneMasterBroadcast", "PhoneSourceIdentity", "PhoneMp3Warmup"]
    tests = ["OwnedPcmMixerTest", "PhoneLiveSessionTest", "OwnedInputPcmTest", "OwnedMediaAudioSinkTest", "OwnedAudioOutputTest", "OwnedSamplerVoicesTest", "PhoneMasterBroadcastTest", "PhoneSourceIdentityTest", "PhoneMp3WarmupTest"]
    files = [source / "main/java/cloud/gitwide/ampverse" / (name + ".java") for name in production]
    files += [source / "test/java/cloud/gitwide/ampverse" / (name + ".java") for name in tests]
    if args.full_cast:
        files.append(root / "android/features/cast/java/cloud/gitwide/ampverse/FullPhoneMp3Encoder.java")
    compile_result = subprocess.run([str(args.java_home / "bin/javac"), "-J-Xmx128m", "--release", "8",
                                     "-cp", classpath, "-d", str(classes), *map(str, files)],
                                    capture_output=True, text=True, timeout=60)
    (args.output / "compile.log").write_text(compile_result.stdout + compile_result.stderr)
    if compile_result.returncode:
        print(compile_result.stderr)
        return compile_result.returncode
    result = subprocess.run([str(args.java_home / "bin/java"), "-Xmx128m", "-cp", classpath,
                             "org.junit.runner.JUnitCore", *["cloud.gitwide.ampverse." + name for name in tests]],
                            capture_output=True, text=True, timeout=60)
    (args.output / "test.log").write_text(result.stdout + result.stderr)
    receipt = {"exit": result.returncode, "heap_mib": 128,
               "scope": "Actual Media3 processors, fake hardware; no Android device/codec/latency acceptance",
               "sources_sha256": {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest() for path in files},
               "dependencies_sha256": {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in jars}}
    (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(result.stdout + result.stderr)
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
