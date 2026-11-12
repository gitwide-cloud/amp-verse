#!/usr/bin/env python3
"""Pinned, checksum-verified official tools for AMP Lab (Windows/Linux x86_64).

No pip, shell, global installation or access credentials. FFmpeg remains the
existing external audio dependency; see docs/LAB_YOUTUBE.md for OS setup.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import tempfile
import urllib.request
import urllib.parse
import zipfile

YT_VERSION = "2026.10.26"
DENO_VERSION = "v2.9.8"


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "AMPVerse-Lab-Setup", "Accept": "application/vnd.github+json"})
    token = os.environ.get("GH_TOKEN")
    if token and urllib.parse.urlsplit(url).hostname == "api.github.com":
        request.add_header("Authorization", "Bearer " + token)
    class SafeRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            redirected = super().redirect_request(req, fp, code, msg, headers, newurl)
            if redirected and urllib.parse.urlsplit(newurl).hostname != urllib.parse.urlsplit(req.full_url).hostname:
                redirected.remove_header("Authorization")
            return redirected
    with urllib.request.build_opener(SafeRedirect).open(request, timeout=120) as response:
        return response.read()


def release(repo, tag):
    return json.loads(fetch(f"https://api.github.com/repos/{repo}/releases/tags/{tag}"))


def verified_asset(info, name):
    assets = {a["name"]: a for a in info["assets"]}
    asset = assets[name]
    content = fetch(asset["browser_download_url"])
    expected = asset.get("digest") or ""
    if expected.startswith("sha256:"):
        expected = expected.removeprefix("sha256:")
    else:
        expected = ""
        for manifest in ("SHA2-256SUMS", name + ".sha256sum"):
            if manifest not in assets:
                continue
            for line in fetch(assets[manifest]["browser_download_url"]).decode().splitlines():
                parts = line.split()
                if len(parts) == 2 and parts[1].lstrip("*") == name:
                    expected = parts[0]
    actual = hashlib.sha256(content).hexdigest()
    if len(expected) != 64 or actual != expected:
        raise RuntimeError(f"Missing or mismatched official SHA256 for {name}; nothing installed")
    return content, {"url": asset["browser_download_url"], "sha256": actual}


def write(path, data, executable=False):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as temp:
        temp.write(data)
        temp.flush()
        os.fsync(temp.fileno())
        temporary = Path(temp.name)
    try:
        temporary.chmod(0o755 if executable else 0o644)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dest", type=Path, required=True, help="AMP Verse/tools/lab-youtube")
    args = parser.parse_args()
    system = platform.system()
    if system not in ("Windows", "Linux") or platform.machine().lower() not in ("amd64", "x86_64"):
        raise SystemExit("Automatic setup supports Windows/Linux x86_64. Install official yt-dlp, EJS, Deno >=2.3 and FFmpeg for your platform, then Locate tools in Lab.")
    windows = system == "Windows"
    yt_name = "yt-dlp.exe" if windows else "yt-dlp_linux"
    deno_name = "deno-x86_64-pc-windows-msvc.zip" if windows else "deno-x86_64-unknown-linux-gnu.zip"
    print("Downloading verified official yt-dlp and Deno releases...", flush=True)
    yt, yt_manifest = verified_asset(release("yt-dlp/yt-dlp", YT_VERSION), yt_name)
    deno_zip, deno_manifest = verified_asset(release("denoland/deno", DENO_VERSION), deno_name)
    binary = "deno.exe" if windows else "deno"
    with zipfile.ZipFile(io.BytesIO(deno_zip)) as archive:
        deno = archive.read(binary)  # Never extract arbitrary archive paths.
    # Collect license and corresponding project sources before publishing binaries.
    notices = {
        "yt-dlp-LICENSE": fetch(f"https://raw.githubusercontent.com/yt-dlp/yt-dlp/{YT_VERSION}/LICENSE"),
        "yt-dlp-THIRD_PARTY_LICENSES.txt": fetch(f"https://raw.githubusercontent.com/yt-dlp/yt-dlp/{YT_VERSION}/THIRD_PARTY_LICENSES.txt"),
        "deno-LICENSE.md": fetch(f"https://raw.githubusercontent.com/denoland/deno/{DENO_VERSION}/LICENSE.md"),
        "deno-rusty-v8-LICENSE": fetch("https://raw.githubusercontent.com/denoland/rusty_v8/v150.4.0/LICENSE"),
        "deno-v8-engine-LICENSE": fetch("https://raw.githubusercontent.com/v8/v8/15.0.245/LICENSE"),
        "deno-typescript-LICENSE": fetch("https://raw.githubusercontent.com/microsoft/TypeScript/v6.0.3/LICENSE.txt"),
        "yt-dlp-source.tar.gz": fetch(f"https://github.com/yt-dlp/yt-dlp/archive/refs/tags/{YT_VERSION}.tar.gz"),
        "deno-source.tar.gz": fetch(f"https://github.com/denoland/deno/archive/refs/tags/{DENO_VERSION}.tar.gz"),
    }
    for name, content in notices.items():
        write(args.dest / "licenses" / name, content)
    write(args.dest / ("yt-dlp.exe" if windows else "yt-dlp"), yt, True)
    write(args.dest / binary, deno, True)
    write(args.dest / "versions.json", json.dumps({"yt-dlp": {"version": YT_VERSION, **yt_manifest}, "deno": {"version": DENO_VERSION, **deno_manifest}, "ejs": "included in official yt-dlp standalone binary", "ffmpeg": "external; existing AMP audio tool lookup"}, indent=2).encode())
    print(f"Installed in {args.dest}. FFmpeg and ffprobe must be beside AMP Verse or on PATH.")


if __name__ == "__main__":
    main()
