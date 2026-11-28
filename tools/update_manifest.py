"""Describe versioned release packages for the in-app updater (no server needed)."""
import argparse
import hashlib
import json
from pathlib import Path
import re


def generate(directory, version, apk=None, version_code=None):
    if not re.fullmatch(r"(?:0|[1-9]\d{0,7})\.(?:0|[1-9]\d{0,7})\.(?:0|[1-9]\d{0,7})(?:-(?:rc|beta)\.[1-9]\d{0,7})?", version):
        raise ValueError("Expected stable, beta or RC version")
    packages = {}
    for platform in ("windows", "linux"):
        path = directory / f"amp-verse-{platform}-v{version}.zip"
        if not path.is_file():
            raise ValueError(f"Missing {platform} package: {path.name}")
        packages[platform + "-x86_64"] = describe(path)
    for platform in ("windows", "linux"):
        path = directory / f"amp-server-{platform}-v{version}.zip"
        if path.is_file():
            packages[f"server-{platform}-x86_64"] = describe(path)
    for platform, name in (("windows", "amp-engine-windows-x86_64.exe"), ("linux", "amp-engine-linux-x86_64")):
        path = directory / name
        if path.is_file():
            packages[f"engine-{platform}-x86_64"] = describe(path)
    if apk:
        if apk.parent.resolve() != directory.resolve() or apk.suffix != ".apk" or not version_code or version_code < 1:
            raise ValueError("APK must be a signed release asset in the package directory with a version code")
        packages["android-arm64"] = {**describe(apk), "version_code": version_code}
    editions = directory / "android-packages.json"
    if editions.is_file():
        android = json.loads(editions.read_text(encoding="utf-8"))
        for key in android["packages"]:
            if key != "android-arm64" and not re.fullmatch(r"android-(?:full|lite|dj|lab|cast|podcasts|remote)-arm64", key):
                raise ValueError("Unknown Android edition")
            entry = android["packages"][key]
            path = directory / entry["name"]
            if path.parent.resolve() != directory.resolve() or not path.is_file() or describe(path) != {field: entry[field] for field in ("name", "sha256", "size")}:
                raise ValueError("Android edition artifact failed verification")
            packages[key] = {**describe(path), "version_code": entry["version_code"]}
    result = {"schema": 1, "version": version, "packages": packages}
    (directory / "amp-update.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def describe(path):
    if not re.fullmatch(r"[A-Za-z0-9._-]+", path.name) or ".." in path.name:
        raise ValueError("Unsafe package name")
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return {"name": path.name, "sha256": digest.hexdigest(), "size": path.stat().st_size}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--apk", type=Path)
    parser.add_argument("--version-code", type=int)
    args = parser.parse_args()
    generate(args.directory, args.version, args.apk, args.version_code)
