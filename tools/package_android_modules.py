"""Package only built, signature-verified APKs; generate the Android module catalog."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import zipfile

IDS = ("dj", "lab", "cast", "podcasts", "remote")
CAPABILITIES = {
    "dj": ["two-deck", "cue-decks", "prepared-auto-mix", "hot-cues", "sampler-playback"],
    "lab": ["trim", "fades", "gain", "normalize", "mp3-export", "youtube-url-import"],
    "cast": ["lan-discovery", "station-listening", "google-cast", "host-live-broadcast", "offline-playlist-sync"],
    "podcasts": ["directory-search", "rss", "preview", "subscriptions", "offline-episodes"],
    "remote": ["host-discovery", "desktop-control", "desktop-library", "desktop-playlists"],
}


def describe(apk):
    return {"name": apk.name, "sha256": hashlib.sha256(apk.read_bytes()).hexdigest(), "size": apk.stat().st_size}


def verified(apk, sdk, java):
    signer = sdk / "build-tools/35.0.0/lib/apksigner.jar"
    result = subprocess.check_output([str(java), "-jar", str(signer), "verify", "--verbose", "--print-certs", str(apk)], text=True)
    certificates = re.findall(r"Signer #\d+ certificate SHA-256 digest: ([a-fA-F0-9]{64})", result)
    if len(certificates) != 1:
        raise ValueError("Expected exactly one APK signer")
    aapt = sdk / ("build-tools/35.0.0/aapt.exe" if os.name == "nt" else "build-tools/35.0.0/aapt")
    badging = subprocess.check_output([str(aapt), "dump", "badging", str(apk)], text=True, encoding="utf-8")
    package = re.search(r"package: name='([^']+)' versionCode='(\d+)' versionName='([^']+)'", badging)
    minimum = re.search(r"sdkVersion:'(\d+)'", badging)
    if not package or not minimum:
        raise ValueError("Cannot verify embedded APK package metadata")
    with zipfile.ZipFile(apk) as archive:
        abis = sorted({name.split("/")[1] for name in archive.namelist() if name.startswith("lib/")})
    return {"package_id": package[1], "version_code": int(package[2]), "version": package[3], "min_sdk": int(minimum[1]), "abis": abis or ["arm64-v8a"], "signer_sha256": certificates[0].lower()}


def package(root, destination, variant, tag, sdk, java):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,100}", tag) or ".." in tag:
        raise ValueError("Invalid release tag")
    destination.mkdir(parents=True, exist_ok=True)
    manifest = {"schema": 1, "api": 1, "android": {}, "editions": {}}
    common_signer = None
    common_version = None
    for module in ("lite", "full", *IDS):
        folder = root / (f"app/build/outputs/apk/{module}/{variant}" if module in ("lite", "full") else f"modules/{module}/build/outputs/apk/{variant}")
        metadata = json.loads((folder / "output-metadata.json").read_text(encoding="utf-8"))
        if len(metadata["elements"]) != 1:
            raise ValueError("Expected one arm64 APK per application")
        apk = (folder / metadata["elements"][0]["outputFile"]).resolve()
        if apk.parent != folder.resolve() or not apk.is_file():
            raise ValueError("APK output is missing or outside its build directory")
        entry = verified(apk, sdk, java)
        expected = "cloud.gitwide.ampverse" + ("" if module in ("lite", "full") else "." + module)
        if entry["package_id"] != expected or not re.fullmatch(r"\d+\.\d+\.\d+", entry["version"]):
            raise ValueError("Unexpected embedded APK identity or version")
        pair = entry["version"], entry["version_code"]
        if common_signer and common_signer != entry["signer_sha256"]:
            raise ValueError("Core and companions must use the same signing identity")
        if common_version and common_version != pair:
            raise ValueError("Editions and companions must be built from one version")
        common_signer, common_version = entry["signer_sha256"], pair
        with zipfile.ZipFile(apk) as archive:
            if module == "lite" and any("libffmpeg" in name or "libpython" in name or "libqjs" in name for name in archive.namelist()):
                raise ValueError("Lite contains editor/downloader native dependencies")
        name = f"amp-verse-{module}-{entry['version']}-b{entry['version_code']}-arm64-v8a.apk"
        output = destination / name
        shutil.copy2(apk, output)
        entry.update(describe(output))
        entry["url"] = f"https://github.com/gitwide-cloud/amp-verse/releases/download/{tag}/{name}"
        entry["api"] = 1
        entry["min_core"] = 34
        if module in IDS:
            aapt = sdk / ("build-tools/35.0.0/aapt.exe" if os.name == "nt" else "build-tools/35.0.0/aapt")
            xml = subprocess.check_output([str(aapt), "dump", "xmltree", str(apk), "AndroidManifest.xml"], text=True, encoding="utf-8")
            for key in ("api", "minCore"):
                match = re.search(r'"amp\.' + key + r'"[^\n]*\n\s+A: android:value[^\n]*\(type 0x10\)0x([0-9a-fA-F]+)', xml)
                if not match:
                    raise ValueError("Companion APK is missing its interface metadata")
                entry["api" if key == "api" else "min_core"] = int(match[1], 16)
            if entry["api"] != 1 or entry["min_core"] > common_version[1]:
                raise ValueError("Companion APK is incompatible with its packaged core")
        entry["release_notes"] = "Android Lite/Full and signed companion integration. See the matching release notes for verified capabilities and limits."
        if module in IDS:
            entry["capabilities"] = CAPABILITIES[module]
            manifest["android"][module] = entry
        else:
            manifest["editions"][module] = entry
        (destination / (name + ".sha256")).write_text(entry["sha256"] + "  " + name + "\n", encoding="utf-8")
    manifest["version"], manifest["version_code"] = common_version
    (destination / "amp-modules.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    packages = {"android-" + edition + "-arm64": entry for edition, entry in manifest["editions"].items()}
    packages["android-arm64"] = packages["android-full-arm64"]
    (destination / "android-packages.json").write_text(json.dumps({"version": common_version[0], "packages": packages}, indent=2) + "\n", encoding="utf-8")
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("android"))
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--variant", choices=["debug", "release"], default="release")
    parser.add_argument("--release-tag", required=True)
    parser.add_argument("--sdk", type=Path, default=Path(os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT", ""))))
    parser.add_argument("--java", type=Path, default=Path(os.environ.get("JAVA_HOME", "")) / ("bin/java.exe" if os.name == "nt" else "bin/java"))
    args = parser.parse_args()
    result = package(args.root, args.destination, args.variant, args.release_tag, args.sdk, args.java)
    print(f"Verified {len(result['editions'])} editions and {len(result['android'])} companion APKs; version {result['version']}")
