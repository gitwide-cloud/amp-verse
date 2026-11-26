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


def product_release(root):
    with (root / "Cargo.toml").open("rb") as source:
        version = tomllib.load(source)["workspace"]["package"]["version"]
    match = re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-rc\.[1-9][0-9]*)?", version)
    if not match:
        raise ValueError("Expected a stable or release candidate product version")
    properties = (root / "android/release.properties").read_text(encoding="utf-8")
    android = dict(line.split("=", 1) for line in properties.splitlines() if line)
    if android.get("versionName") != version:
        raise ValueError("Desktop and Android product versions disagree")
    channel = "rc" if "-rc." in version else "stable"
    return version, channel


def desktop_version(root):
    version, _ = product_release(root)
    ref = os.environ.get("GITHUB_REF", "")
    if ref.startswith("refs/tags/v"):
        if ref.removeprefix("refs/tags/v") != version:
            raise ValueError("Release tag does not match product version")
        return version
    build = os.environ.get("GITHUB_SHA", "local")[:8]
    return safe(f"{version}+git.{safe(build)}")


def write_desktop_manifest(root, output):
    version, channel = product_release(root)
    manifest = {
        "source_sha": os.environ["GITHUB_SHA"],
        "version": version,
        "release_channel": channel,
        "package_version": desktop_version(root),
    }
    output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


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
    manifest = commands.add_parser("desktop-manifest")
    manifest.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    manifest.add_argument("--output", type=Path, required=True)
    android = commands.add_parser("android")
    android.add_argument("--source", type=Path, required=True)
    android.add_argument("--destination", type=Path, required=True)
    android.add_argument("--build", default=os.environ.get("GITHUB_SHA", ""))
    args = parser.parse_args()
    if args.command == "desktop-version":
        print(desktop_version(args.root))
    elif args.command == "desktop-manifest":
        write_desktop_manifest(args.root, args.output)
    else:
        print("version=" + package_android(args.source, args.destination, args.build))
