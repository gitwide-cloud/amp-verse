"""Wrap a checksum-verified Windows role package in an offline, per-user setup EXE."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verified_package(package):
    manifest = json.loads((package / "install-manifest.json").read_text(encoding="utf-8-sig"))
    if manifest.get("schema") != 1 or manifest.get("role") != "Desktop":
        raise ValueError("A verified Desktop role package is required")
    if sha(package / "install-windows-package.ps1") != manifest["bootstrap_sha256"]:
        raise ValueError("Installer script checksum mismatch")
    for directory, field in [("payload", "files"), ("support", "support_files")]:
        root = (package / directory).resolve()
        expected = set()
        for entry in manifest[field]:
            relative = Path(entry["path"])
            target = root / relative
            if relative.is_absolute() or ".." in relative.parts or ":" in str(relative) or not target.resolve().is_relative_to(root):
                raise ValueError("Unsafe installer path")
            if target.is_symlink() or not target.is_file() or sha(target) != entry["sha256"]:
                raise ValueError("Installer payload checksum mismatch: " + entry["path"])
            expected.add(target.resolve())
        if {p.resolve() for p in root.rglob("*") if p.is_file()} != expected:
            raise ValueError("Installer inventory does not cover its payload")
    return manifest


def package_exe(package, output, compiler):
    package = package.resolve()
    if output.exists():
        raise ValueError("Keep the existing installer; choose a new output path")
    manifest = verified_package(package)
    build = json.loads((package / "payload/build.json").read_text(encoding="utf-8-sig"))
    if build.get("source_sha") != manifest["source_commit"]:
        raise ValueError("Build and installer source identities differ")
    version = build["version"]
    if any(c not in "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-+" for c in version):
        raise ValueError("Invalid build version")
    source_file = Path(__file__).with_name("WindowsInstaller.cs")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="amp-setup-build-", dir=output.parent) as temporary:
        temporary = Path(temporary)
        archive = temporary / "package.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as zipped:
            for path in sorted(package.rglob("*")):
                if path.is_symlink():
                    raise ValueError("Linked package input refused")
                if path.is_file():
                    zipped.write(path, path.relative_to(package).as_posix())
        archive_hash = sha(archive)
        source = source_file.read_text(encoding="utf-8")
        for key, value in {"PACKAGE_SHA256": archive_hash, "SCRIPT_SHA256": manifest["bootstrap_sha256"], "BUILD": manifest["source_commit"], "VERSION": version}.items():
            source = source.replace("@@" + key + "@@", value)
        if "@@" in source:
            raise ValueError("Unresolved setup template input")
        compiled_source = temporary / "Installer.cs"
        compiled_source.write_text(source, encoding="utf-8")
        result = subprocess.run([str(compiler), "/nologo", "/target:winexe", "/platform:x64", "/optimize+", "/reference:System.Windows.Forms.dll", "/reference:System.Drawing.dll", "/reference:System.IO.Compression.dll", "/resource:" + str(archive) + ",amp-package.zip", "/out:" + str(output), str(compiled_source)], text=True, capture_output=True)
        if result.returncode:
            raise RuntimeError("Setup compilation failed:\n" + result.stdout + result.stderr)
    receipt = {"schema": 1, "role": "Desktop", "application_source_commit": manifest["source_commit"], "run_id": manifest["run_id"], "version": version, "sha256": sha(output), "bytes": output.stat().st_size, "embedded_package_sha256": archive_hash, "install_manifest_sha256": sha(package / "install-manifest.json"), "wrapper_source_sha256": sha(source_file), "packager_sha256": sha(Path(__file__)), "code_signed": False}
    output.with_suffix(".json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    output.with_suffix(".exe.sha256").write_text(receipt["sha256"] + "  " + output.name + "\n", encoding="utf-8")
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--compiler", type=Path, default=Path(os.environ.get("WINDIR", "C:/Windows")) / "Microsoft.NET/Framework64/v4.0.30319/csc.exe")
    args = parser.parse_args()
    print(json.dumps(package_exe(args.package, args.output, args.compiler)))
