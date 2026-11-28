"""Write component compatibility metadata alongside a built engine and checksum."""
import argparse
import json
from pathlib import Path
import re


def write_metadata(root, directory, platform, release):
    if not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-(?:beta|rc)\.[1-9][0-9]*)?", release):
        raise ValueError("Expected a stable, beta or RC release tag")
    asset = {"windows-x86_64": "amp-engine-windows-x86_64.exe",
             "linux-x86_64": "amp-engine-linux-x86_64"}[platform]
    source = (root / "crates/amp-core/src/engine_api.rs").read_text()
    protocol = int(re.search(r"pub const PROTOCOL: u32 = (\d+);", source)[1])
    engine_id = re.search(r'pub const ENGINE_ID: &str = "([^"]+)";', source)[1]
    if not (directory / asset).is_file() or not (directory / (asset + ".sha256")).is_file():
        raise ValueError("Build and checksum must exist before metadata generation")
    metadata = dict(schema=1, component="amp-engine", release=release,
                    platform=platform, engine_id=engine_id, protocol=protocol, asset=asset)
    (directory / (asset + ".json")).write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--platform", choices=["windows-x86_64", "linux-x86_64"], required=True)
    parser.add_argument("--release", required=True)
    args = parser.parse_args()
    write_metadata(Path(__file__).resolve().parents[1], args.directory, args.platform, args.release)
