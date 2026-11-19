"""Compile the real std-only chroma module and compare it with its preserved reference.
Uses generated in-memory fixtures, not user music. Reports component-only warm timing.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    source = root / "crates/amp-core/src/analysis_chroma.rs"
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=True)
    harness = output / "harness.rs"
    harness.write_text('const RATE: f64 = 11025.;\nconst FFT: usize = 4096;\n#[path = r"' + str(source) + '"]\nmod chroma;\n')
    binary = output / ("chroma-check.exe" if os.name == "nt" else "chroma-check")
    commands = [
        ["rustc", "--edition=2024", "-O", "--test", str(harness), "-o", str(binary)],
        [str(binary), "--nocapture"],
        [str(binary), "--ignored", "--exact", "chroma::tests::benchmark_original_and_reused_scratch", "--nocapture"],
    ]
    logs = []
    for command in commands:
        result = subprocess.run(command, capture_output=True, text=True, timeout=120)
        logs.append("$ " + " ".join(command) + "\n" + result.stdout + result.stderr)
        (output / "verification.log").write_text("\n".join(logs))
        if result.returncode:
            print(result.stdout + result.stderr)
            raise SystemExit(result.returncode)
    match = re.search(r"chroma benchmark: frames=(\d+) original_us=(\d+) reused_us=(\d+) speedup=([\d.]+)x", logs[-1])
    if match is None:
        raise RuntimeError("Actual benchmark result missing")
    frames, original, reused, speedup = match.groups()
    report = {
        "commit": os.environ.get("GITHUB_SHA", "local"),
        "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "platform": os.name,
        "scope": "warm chroma component only; excludes initialization, decode, rhythm, network and storage",
        "fixtures": "generated silence/chord/noise/impulse/short/quiet/long frames",
        "reference_bit_parity": True,
        "independent_worker_test": True,
        "frames": int(frames), "original_us": int(original), "reused_us": int(reused),
        "component_speedup": float(speedup),
        "timing_acceptance_threshold": None,
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
