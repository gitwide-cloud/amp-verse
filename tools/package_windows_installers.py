"""Package verified Windows role payloads with offline runtimes; never installs or starts apps."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import zipfile
from setup_ci_ffmpeg import verify_bytes, selected_binaries, ARCHIVE_SHA256, ARCHIVE_SIZE, VERSION

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open("rb") as source:
        value = hashlib.sha256()
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def copy_tree(source, target):
    if source.is_symlink() or not source.is_dir():
        raise ValueError("Application input must be a real extracted directory")
    for path in sorted(source.rglob("*")):
        if path.is_symlink():
            raise ValueError("Linked input is unsupported")
        if path.is_file():
            destination = target / path.relative_to(source)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, destination)


def inventory(root):
    return [{"path": p.relative_to(root).as_posix(), "sha256": digest(p)}
            for p in sorted(root.rglob("*")) if p.is_file()]


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def package(role, application, engine, archive, crt, output, run_id):
    if output.exists():
        raise ValueError("Keep earlier evidence; output must not exist")
    if not str(run_id).isdigit():
        raise ValueError("A verified input CI run ID is required")
    metadata = json.loads((application / "build.json").read_text(encoding="utf-8-sig"))
    commit = metadata.get("source_sha", metadata.get("commit", ""))
    if len(commit) != 40 or any(c not in "0123456789abcdef" for c in commit):
        raise ValueError("Application build lacks its source identity")
    data = verify_bytes(archive.read_bytes())
    crt_receipt = json.loads((crt / 'CRT-PROVENANCE.json').read_text(encoding='utf-8-sig'))
    if not {'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'}.issubset(
            {item['name'] for item in crt_receipt['files']}):
        raise ValueError('Complete app-local CRT dependencies are required')
    if digest(crt / 'notices/vc_redist.x64.exe') != crt_receipt['original_redist_sha256']:
        raise ValueError('Original Microsoft redist/license container changed')
    for item in crt_receipt['files']:
        if Path(item['name']).name != item['name'] or not item['name'].endswith('.dll'):
            raise ValueError('Unsafe CRT receipt name')
        if digest(crt / item['name']) != item['sha256'] or 'O=Microsoft Corporation' not in item['signer']:
            raise ValueError('CRT payload differs from its Microsoft-verified export')
    payload = output / "payload"
    support = output / "support"
    payload.mkdir(parents=True)
    support.mkdir()
    copy_tree(application, payload)
    update_files = inventory(payload)
    if role == "Desktop":
        if engine is None:
            raise ValueError("The matching analysis-engine artifact is required")
        binary = engine / "amp-engine-windows-x86_64.exe"
        if not binary.is_file() or binary.is_symlink():
            raise ValueError("Matching engine binary is missing")
        # The parent delivery pins the same-run engine; retain its original bytes.
        (payload / "addons").mkdir(exist_ok=True)
        shutil.copy2(binary, payload / "addons/amp-engine.exe")
        (support / "engine").mkdir()
        shutil.copy2(binary, support / "engine" / binary.name)
        shutil.copy2(ROOT / "tools/install-tested-desktop.ps1", support / "install-tested-desktop.ps1")
        write_json(support / "delivery-manifest.json", {
            "version": 1, "source_commit": commit, "desktop_run_id": str(run_id),
            "engine_run_id": str(run_id), "desktop_files": update_files,
            "engine_files": [{"path": binary.name, "sha256": digest(binary)}],
        })
    notice_root = payload / "licenses/ffmpeg"
    notice_root.mkdir(parents=True, exist_ok=True)
    for name, content in selected_binaries(data):
        (payload / name).write_bytes(content)
    for item in crt_receipt['files']:
        shutil.copy2(crt / item['name'], payload / item['name'])
    copy_tree(crt / 'notices', payload / 'licenses/msvc')
    shutil.copy2(crt / 'CRT-PROVENANCE.json', payload / 'licenses/msvc/CRT-PROVENANCE.json')
    with zipfile.ZipFile(archive) as original:
        for name in ("LICENSE", "README.txt"):
            (notice_root / name).write_bytes(original.read("ffmpeg-9.1-essentials_build/" + name))
    write_json(payload / "AUDIO_TOOL_PROVENANCE.json", {
        "version": VERSION, "archive_sha256": ARCHIVE_SHA256, "archive_bytes": ARCHIVE_SIZE,
        "source_url": "https://github.com/GyanD/codexffmpeg/releases/download/9.1/ffmpeg-9.1-essentials_build.zip",
        "publisher_reference": "https://ffmpeg.org/download.html#build-windows",
        "binaries": {name: digest(payload / name) for name in ("ffmpeg.exe", "ffprobe.exe", "ffplay.exe")},
    })
    (payload / "AUDIO_TOOL_NOTICES.md").write_text(
        "FFmpeg " + VERSION + " essentials build by Gyan.\n"
        "These separate executables are distributed under the upstream GPLv3 terms.\n"
        "The original LICENSE and builder README are retained in licenses/ffmpeg.\n"
        "AUDIO_TOOL_PROVENANCE.json records the pinned archive and binary hashes.\n",
        encoding="utf-8")
    shutil.copy2(ROOT / "tools/launch-windows-installed.ps1", payload / "launch-windows-installed.ps1")
    shutil.copy2(ROOT / "tools/install-windows-package.ps1", output / "install-windows-package.ps1")
    entry = '@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -STA -File "%~dp0install-windows-package.ps1" -PackageRoot "%~dp0." -CreateShortcuts\r\npause\r\n'
    (output / "Install AMP Verse.cmd").write_bytes(entry.encode())
    modes = {"Launch AMP Verse.cmd": "Window"} if role == "Desktop" else {
        "Open AMP Verse Server.cmd": "Window", "Server Tray.cmd": "Tray",
        "Run Server Console.cmd": "Console"}
    for name, mode in modes.items():
        (payload / name).write_bytes(('@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0launch-windows-installed.ps1" -Mode ' + mode + '\r\n').encode())
    if role == "Server":
        (payload / "Start AMP Verse Server.ps1").write_text(
            '$ErrorActionPreference="Stop"\n& (Join-Path $PSScriptRoot "launch-windows-installed.ps1") -Mode Background\n',
            encoding="utf-8")
        shutil.copy2(ROOT / "deploy/show-windows-server-access.ps1", payload / "show-windows-server-access.ps1")
        # Preserve server updater's root inventory while refreshing this packaging-only helper.
        for item in metadata["files"]:
            if item["name"] == "show-windows-server-access.ps1":
                item["size"] = (payload / item["name"]).stat().st_size
                item["sha256"] = digest(payload / item["name"])
        write_json(payload / "build.json", metadata)
    installer = {
        "schema": 1, "role": role, "source_commit": commit, "run_id": str(run_id),
        "files": inventory(payload), "support_files": inventory(support),
        "packager_sha256": digest(Path(__file__)),
        "bootstrap_sha256": digest(output / 'install-windows-package.ps1'),
        "scope": "Application source is identified above; installer scripts are independently hashed packaging inputs.",
    }
    write_json(output / "install-manifest.json", installer)
    (output / "INSTALL.md").write_text(
        "# AMP Verse Windows " + role + "\n\n"
        "Verify the downloaded archive against the published checksum, extract it locally, then open Install AMP Verse.cmd.\n"
        "Choose an existing music folder. No Python, Rust, automatic service, firewall change or Windows startup task is used.\n"
        "The installer creates a single canonical role installation. Existing installs use their preserving updater.\n"
        "Fresh server controls are code-free by default. -ProtectedMode explicitly enables a private access code.\n"
        "The server GUI needs Microsoft Edge WebView2; Run Server Console.cmd remains headless and independent.\n"
        "Nothing starts during installation unless -StartAfterInstall is explicitly supplied.\n"
        "PowerShell: .\\install-windows-package.ps1 -MusicDirectory 'D:\\Music' -NoStart\n",
        encoding="utf-8")
    return installer


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--role", choices=["Desktop", "Server"], required=True)
    parser.add_argument("--application", type=Path, required=True)
    parser.add_argument("--engine", type=Path)
    parser.add_argument("--codec-archive", type=Path, required=True)
    parser.add_argument("--crt-directory", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    package(args.role, args.application, args.engine, args.codec_archive, args.crt_directory, args.output, args.run_id)
