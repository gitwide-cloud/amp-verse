"""Package CI-built unsigned betas for signing locally with the existing key.
Never reads or uploads a private signing key. Signed tag releases use the existing
release pipeline; this artifact is deliberately not an installable update yet.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def package(root, destination, sdk):
    destination.mkdir(parents=True, exist_ok=True)
    tools = sdk / "build-tools/35.0.0"
    signer = tools / "lib/apksigner.jar"
    if not signer.is_file():
        raise ValueError("Official installed build-tools 35 apksigner.jar is missing")
    local_tools = destination / "signing-tools"
    local_tools.mkdir(exist_ok=True)
    shutil.copy2(signer, local_tools / "apksigner.jar")
    if (tools / "NOTICE.txt").is_file():
        shutil.copy2(tools / "NOTICE.txt", local_tools / "NOTICE.txt")
    report = {"schema": 1, "kind": "unsigned-beta-requires-local-signing", "source_commit": os.environ.get("GITHUB_SHA", "local"), "build_tools": "35.0.0", "release_channel": "beta", "beta": 1, "packages": []}
    common_version = None
    for module in ("lite", "full", "dj", "lab", "cast", "podcasts", "remote"):
        folder = root / (f"app/build/outputs/apk/{module}/release" if module in ("lite", "full") else f"modules/{module}/build/outputs/apk/release")
        metadata_file = folder / "output-metadata.json"
        metadata = json.loads(metadata_file.read_text())
        if len(metadata["elements"]) != 1:
            raise ValueError("Expected one arm64 release APK per edition/module")
        apk = (folder / metadata["elements"][0]["outputFile"]).resolve()
        if apk.parent != folder.resolve() or not apk.name.endswith("-unsigned.apk") or not apk.is_file():
            raise ValueError("Expected an unsigned release APK inside its output folder")
        badging = subprocess.check_output([str(tools / "aapt"), "dump", "badging", str(apk)], text=True)
        identity = re.search(r"package: name='([^']+)' versionCode='(\d+)' versionName='([^']+)'", badging)
        if not identity:
            raise ValueError("Could not inspect embedded APK identity")
        package_id, version_code, version = identity.groups()
        expected = "cloud.gitwide.ampverse" + ("" if module in ("lite", "full") else "." + module)
        if package_id != expected or not re.fullmatch(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-beta\.[1-9]\d*)?", version):
            raise ValueError("Unexpected APK package identity/version")
        pair = version, int(version_code)
        if common_version is not None and pair != common_version:
            raise ValueError("Editions and modules must share one version")
        common_version = pair
        aligned = subprocess.run([str(tools / "zipalign"), "-c", "-P", "16", "4", str(apk)], capture_output=True, text=True)
        if aligned.returncode:
            raise ValueError("Unsigned release APK failed alignment verification: " + aligned.stderr)
        name = f"amp-verse-{module}-{version}-b{version_code}-arm64-v8a-unsigned.apk"
        target = destination / name
        shutil.copy2(apk, target)
        (destination / (name + ".badging.txt")).write_text(badging)
        (destination / (name + ".output-metadata.json")).write_text(json.dumps(metadata, indent=2) + "\n")
        (destination / (name + ".alignment.txt")).write_text("PASS: official build-tools 35 zipalign -c -P 16 4\n" + aligned.stdout + aligned.stderr)
        report["packages"].append({"edition": module, "name": name, "package_id": package_id, "version_code": int(version_code), "version": version, "sha256": hashlib.sha256(target.read_bytes()).hexdigest(), "size": target.stat().st_size, "alignment_verified": True, "signed": False})
    (destination / "unsigned-manifest.json").write_text(json.dumps(report, indent=2) + "\n")
    (destination / "SIGNING_REQUIRED.md").write_text("""# Unsigned beta — local signing required

These APKs cannot be installed until signed. Keep the existing private keystore
on the owner's computer; it was not uploaded to this build. Use the included
official build-tools 35 apksigner.jar with Java to sign copies of these files,
then verify their signatures and compare the signer certificate with the
existing installed/published app before distributing an update. Use the same
signer for core and companion APKs. Do not uninstall the existing app or clear
its data to work around a certificate mismatch.

Embedded package/version reports, Gradle output metadata and alignment checks
are included. Regenerate SHA-256 checksums after local signing; the unsigned
checksums do not describe the signed APKs. This workflow does not publish a
release or upload an Android private key.
""")
    checksums = []
    for file in sorted(destination.rglob("*")):
        if file.is_file() and file.name != "SHA256SUMS.txt":
            checksums.append(hashlib.sha256(file.read_bytes()).hexdigest() + "  " + file.relative_to(destination).as_posix())
    (destination / "SHA256SUMS.txt").write_text("\n".join(checksums) + "\n")
    print(f"Prepared {len(report['packages'])} aligned unsigned APKs for local signing")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("android"))
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, default=Path(os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT", ""))))
    args = parser.parse_args()
    package(args.root, args.destination, args.sdk)
