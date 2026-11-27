"""Package exact-source unsigned RELEASE APKs; private keys never enter CI."""
import argparse, hashlib, json, os, re, shutil, subprocess, zipfile
from pathlib import Path

MODULES = ("lite", "full", "dj", "lab", "cast", "podcasts", "remote")

def identity(badging, package_id, version, code):
    match = re.search(r"package: name='([^']+)' versionCode='(\d+)' versionName='([^']+)'", badging)
    if not match or match.groups() != (package_id, str(code), version):
        raise ValueError("Embedded release package/version differs from the approved source")
    if "application-debuggable" in badging:
        raise ValueError("Debuggable APK cannot become the locally signed release")
    return dict(package_id=package_id, version=version, version_code=code, debuggable=False)

def package(source, destination, sdk, expected):
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if actual != expected or not re.fullmatch("[0-9a-f]{40}", expected):
        raise ValueError("Unsigned APK source must match the approved commit")
    subprocess.run(["git", "diff", "--quiet", "HEAD", "--"], cwd=source, check=True)
    properties = dict(line.split("=",1) for line in (source/"android/release.properties").read_text().splitlines() if line and not line.startswith("#"))
    version, code = properties["versionName"], int(properties["versionCode"])
    if not re.fullmatch(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:rc|beta)\.[1-9]\d*)?", version):
        raise ValueError("Unsupported embedded release version")
    tools=sdk/"build-tools/35.0.0"
    destination.mkdir(parents=True,exist_ok=True)
    signer=tools/"lib/apksigner.jar"
    shutil.copyfile(signer,destination/"apksigner.jar")
    report=dict(schema=1, kind="unsigned-release-for-local-signing", source_commit=actual, version=version, version_code=code, private_key_in_ci=False, signer_tool_sha256=hashlib.sha256(signer.read_bytes()).hexdigest(), packages=[])
    for module in MODULES:
        folder=source/(f"android/app/build/outputs/apk/{module}/release" if module in ("lite","full") else f"android/modules/{module}/build/outputs/apk/release")
        metadata=json.loads((folder/"output-metadata.json").read_text())
        if len(metadata["elements"])!=1: raise ValueError("Expected one arm64 release APK")
        apk=(folder/metadata["elements"][0]["outputFile"]).resolve()
        if apk.parent!=folder.resolve() or not apk.name.endswith("-unsigned.apk"): raise ValueError("Expected actual unsigned release output")
        badging=subprocess.check_output([str(tools/"aapt"),"dump","badging",str(apk)],text=True)
        entry=identity(badging,"cloud.gitwide.ampverse"+("" if module in ("lite","full") else "."+module),version,code)
        subprocess.run([str(tools/"zipalign"),"-c","-P","16","4",str(apk)],check=True)
        with zipfile.ZipFile(apk) as archive:
            abis={n.split("/")[1] for n in archive.namelist() if n.startswith("lib/") and len(n.split("/"))>2}
        if abis-{"arm64-v8a"} or (module in ("lite","full") and abis!={"arm64-v8a"}): raise ValueError("Unexpected release native ABI set")
        name=f"amp-verse-{module}-{version}-b{code}-arm64-v8a-unsigned.apk"
        shutil.copyfile(apk,destination/name)
        (destination/(name+".badging.txt")).write_text(badging)
        entry.update(edition=module,name=name,sha256=hashlib.sha256(apk.read_bytes()).hexdigest(),bytes=apk.stat().st_size,alignment_verified=True,signed=False)
        report["packages"].append(entry)
    (destination/"unsigned-manifest.json").write_text(json.dumps(report,indent=2)+"\n")
    (destination/"SHA256SUMS.txt").write_text("".join(hashlib.sha256(p.read_bytes()).hexdigest()+"  "+p.name+"\n" for p in sorted(destination.iterdir()) if p.name!="SHA256SUMS.txt"))

if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source",type=Path,required=True);parser.add_argument("--destination",type=Path,required=True)
    parser.add_argument("--expected-source",required=True);parser.add_argument("--sdk",type=Path,default=Path(os.environ.get("ANDROID_HOME","")))
    args=parser.parse_args();package(args.source,args.destination,args.sdk,args.expected_source)
