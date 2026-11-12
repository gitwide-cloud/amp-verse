"""Version filenames of public downloads without renaming installed executables."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import tomllib


def safe(value):
    value = str(value)
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9.+_-]{0,100}", value) or ".." in value:
        raise ValueError("Invalid package version or build identifier")
    return value


def desktop_version(root):
    ref = os.environ.get("GITHUB_REF", "")
    if ref.startswith("refs/tags/v"):
        return safe(ref.removeprefix("refs/tags/v"))
    with (root / "Cargo.toml").open("rb") as source:
        version = tomllib.load(source)["workspace"]["package"]["version"]
    build = os.environ.get("GITHUB_SHA", "local")[:8]
    return safe(f"{version}-dev.{safe(build)}")


def package_android(source, destination, build):
    metadata = json.loads((source / "output-metadata.json").read_text(encoding="utf-8"))
    variant = safe(metadata["variantName"])
    destination.mkdir(parents=True, exist_ok=True)
    versions = set()
    names = set()
    for output in metadata["elements"]:
        version = safe(output["versionName"])
        code = int(output["versionCode"])
        if code < 1:
            raise ValueError("Invalid Android version code")
        original = (source / output["outputFile"]).resolve()
        if original.parent != source.resolve() or original.suffix != ".apk":
            raise ValueError("APK must be inside the build output directory")
        filters = "".join("-" + safe(item["identifier"]) for item in output.get("filters", []))
        suffix = "-" + safe(build[:8]) if build else ""
        name = f"amp-verse-{version}-b{code}-{variant}{filters}{suffix}.apk"
        if name in names:
            raise ValueError("APK outputs would have identical names")
        names.add(name)
        shutil.copy2(original, destination / name)
        versions.add(version)
    if len(versions) != 1:
        raise ValueError("Expected one app version per package")
    return versions.pop()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    desktop = commands.add_parser("desktop-version")
    desktop.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    android = commands.add_parser("android")
    android.add_argument("--source", type=Path, required=True)
    android.add_argument("--destination", type=Path, required=True)
    android.add_argument("--build", default=os.environ.get("GITHUB_SHA", ""))
    args = parser.parse_args()
    if args.command == "desktop-version":
        print(desktop_version(args.root))
    else:
        print("version=" + package_android(args.source, args.destination, args.build))
