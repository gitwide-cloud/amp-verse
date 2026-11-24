"""Guard persistent-key Android RC builds and describe their verified APKs.

No signing key is read by this helper. The workflow introduces signing secrets
only after this source/native gate succeeds.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.request

NATIVE_WORKFLOW = ".github/workflows/android-owned-pcm-native.yml"
RUNTIME_PATHS = ["android", "crates", "apps/amp-cast", "Cargo.toml", "Cargo.lock",
                 "tools/build_android_dj.py", "tools/package_android_modules.py",
                 "tools/verify_android_owned_native.py", NATIVE_WORKFLOW]


def identity(properties, ref, requested=""):
    values = dict(line.split("=", 1) for line in properties.splitlines() if line and not line.startswith("#"))
    version = values.get("versionName", "")
    match = re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-rc\.([1-9]\d*)", version)
    if not match or not values.get("versionCode", "").isdigit() or int(values["versionCode"]) < 1:
        raise ValueError("A positive, embedded RC identity is required")
    tag = "v" + version
    build_tag = "android-rc-build-" + tag
    if ref.startswith("refs/tags/"):
        if ref not in ("refs/tags/" + tag, "refs/tags/" + build_tag):
            raise ValueError("RC tag does not match the embedded application version")
    elif ref != "refs/heads/main" and not ref.startswith("refs/heads/release/"):
        raise ValueError("Dispatch signed RC builds only from the reviewed release branch")
    if requested and requested != tag:
        raise ValueError("Requested RC tag does not match the embedded application version")
    return {"version": version, "version_code": int(values["versionCode"]), "tag": tag,
            "rc": int(match[4]), "artifact_only": ref == "refs/tags/" + build_tag}


def native_matches(run, root, tested_sha):
    if (run.get("status") != "completed" or run.get("conclusion") != "success"
            or run.get("path") != NATIVE_WORKFLOW
            or run.get("repository", {}).get("full_name") != "gitwide-cloud/amp-verse"
            or not re.fullmatch(r"[0-9a-f]{40}", run.get("head_sha", ""))):
        return False
    sha = run["head_sha"]
    if not re.fullmatch(r"[0-9a-f]{40}", tested_sha):
        return False
    present = subprocess.run(["git", "cat-file", "-e", tested_sha + "^{commit}"], cwd=root,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if present.returncode:
        return False
    if run.get("event") == "pull_request":
        parents = subprocess.check_output(["git", "rev-list", "--parents", "-n", "1", tested_sha], cwd=root, text=True).split()
        if len(parents) != 3 or parents[2] != sha:
            return False
    elif tested_sha != sha:
        return False
    verifier = subprocess.run(["git", "diff", "--quiet", sha, tested_sha, "--", NATIVE_WORKFLOW,
                               "tools/verify_android_owned_native.py", "android/app/src/androidTest"], cwd=root)
    if verifier.returncode:
        return False
    result = subprocess.run(["git", "diff", "--quiet", tested_sha, "HEAD", "--", *RUNTIME_PATHS], cwd=root)
    return result.returncode == 0


def github(path):
    token = os.environ.get("GH_TOKEN", "")
    if not token:
        raise ValueError("GitHub workflow token is required for native gate verification")
    request = urllib.request.Request("https://api.github.com/repos/gitwide-cloud/amp-verse/" + path,
                                     headers={"Authorization": "Bearer " + token,
                                              "Accept": "application/vnd.github+json",
                                              "X-GitHub-Api-Version": "2022-11-28"})
    with urllib.request.urlopen(request, timeout=20) as response:
        payload = response.read(2 * 1024 * 1024 + 1)
    if len(payload) > 2 * 1024 * 1024:
        raise ValueError("GitHub gate response exceeds its limit")
    return json.loads(payload)


def gate(root, ref, requested, native_run, output):
    result = identity((root / "android/release.properties").read_text(), ref, requested)
    clean = subprocess.run(["git", "diff", "--quiet", "HEAD", "--", *RUNTIME_PATHS], cwd=root)
    if clean.returncode:
        raise ValueError("Signing requires a clean reviewed runtime checkout")
    branches = subprocess.check_output(["git", "branch", "-r", "--contains", "HEAD"], cwd=root, text=True)
    if not any(line.strip() == "origin/main" or line.strip().startswith("origin/release/") for line in branches.splitlines()):
        raise ValueError("The signed source must already be on a reviewed canonical release branch")
    if native_run:
        if not re.fullmatch(r"[1-9]\d{0,19}", native_run):
            raise ValueError("Invalid native run ID")
        candidates = [github("actions/runs/" + native_run)]
    else:
        candidates = github("actions/workflows/android-owned-pcm-native.yml/runs?status=success&per_page=30")["workflow_runs"]
    matched = None
    tested_source = None
    for run in candidates:
        if run.get("status") != "completed" or run.get("conclusion") != "success" or run.get("path") != NATIVE_WORKFLOW:
            continue
        artifacts = github("actions/runs/" + str(run["id"]) + "/artifacts?per_page=100")["artifacts"]
        for artifact in artifacts:
            match = re.fullmatch(r"android-owned-native-([0-9a-f]{40})", artifact.get("name", ""))
            if artifact.get("expired") or not match:
                continue
            tested = match[1]
            present = subprocess.run(["git", "cat-file", "-e", tested + "^{commit}"], cwd=root, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if present.returncode:
                subprocess.run(["git", "fetch", "--no-tags", "origin", tested], cwd=root, check=True, timeout=60)
            if native_matches(run, root, tested):
                matched, tested_source = run, tested
                break
        if matched is not None:
            break
    if matched is None:
        raise ValueError("No successful native Android run matches this runtime source; qualify it before signing")
    result.update(source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
                  native_run_id=matched["id"], native_head_sha=matched["head_sha"], native_tested_source=tested_source,
                  native_run_url=matched.get("html_url", ""), release_channel="rc")
    output.write_text(json.dumps(result, indent=2) + "\n")
    github_output = os.environ.get("GITHUB_OUTPUT")
    if github_output:
        with open(github_output, "a") as file:
            for name in ("tag", "version", "version_code", "artifact_only"):
                file.write(f"{name}={str(result[name]).lower()}\n")
    return result


def manifest(directory, gate_file):
    proof = json.loads(gate_file.read_text())
    packages = json.loads((directory / "android-packages.json").read_text())
    if packages["version"] != proof["version"]:
        raise ValueError("Signed package version differs from the reviewed RC")
    signers = set()
    for item in packages["packages"].values():
        path = directory / item["name"]
        if path.parent.resolve() != directory.resolve() or not path.is_file():
            raise ValueError("Signed APK is missing or outside its artifact folder")
        if (item["version"] != proof["version"] or item["version_code"] != proof["version_code"]
                or hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]):
            raise ValueError("Signed APK identity/hash differs from its verified metadata")
        signers.add(item["signer_sha256"])
    if len(signers) != 1:
        raise ValueError("RC editions must share the persistent signing identity")
    result = {"schema": 1, "version": proof["version"], "release_channel": "rc", "rc": proof["rc"],
              "source_commit": proof["source_commit"], "native_run_id": proof["native_run_id"],
              "packages": packages["packages"]}
    (directory / "amp-update.json").write_text(json.dumps(result, indent=2) + "\n")
    (directory / "android-rc-gate.json").write_text(json.dumps(proof, indent=2) + "\n")
    sums = []
    for path in sorted(directory.iterdir()):
        if path.is_file() and path.name != "SHA256SUMS.txt":
            sums.append(hashlib.sha256(path.read_bytes()).hexdigest() + "  " + path.name)
    (directory / "SHA256SUMS.txt").write_text("\n".join(sums) + "\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    first = sub.add_parser("gate")
    first.add_argument("--root", type=Path, default=Path.cwd())
    first.add_argument("--ref", required=True)
    first.add_argument("--tag", default="")
    first.add_argument("--native-run", default="")
    first.add_argument("--output", type=Path, required=True)
    second = sub.add_parser("manifest")
    second.add_argument("--directory", type=Path, required=True)
    second.add_argument("--gate-file", type=Path, required=True)
    args = parser.parse_args()
    if args.operation == "gate":
        print(json.dumps(gate(args.root, args.ref, args.tag, args.native_run, args.output), indent=2))
    else:
        print(json.dumps(manifest(args.directory, args.gate_file), indent=2))
