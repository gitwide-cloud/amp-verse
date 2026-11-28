# Build state-free server-core packages for the in-app transaction.
from pathlib import Path
import argparse, json, zipfile, stat

def package(source, destination):
    build = json.loads((source / "build.json").read_text(encoding="utf-8-sig"))
    windows = (source / "amp-cast.exe").is_file()
    suffix = ".exe" if windows else ""
    names = ["amp-cast"+suffix, "amp-server-update"+suffix, "amp-prepare"+suffix, "build.json", "LICENSE", "THIRD_PARTY_NOTICES.md"]
    engine = "amp-engine"+suffix
    if (source / engine).is_file(): names.append(engine)
    for name in names:
        path=source/name
        if not path.is_file() or path.is_symlink(): raise ValueError("Incomplete server update: "+name)
    if not (build.get("source_sha") or build.get("commit")): raise ValueError("Server source identity missing")
    destination.parent.mkdir(parents=True,exist_ok=True)
    with zipfile.ZipFile(destination,"w",zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
        for name in names:
            path=source/name
            info=zipfile.ZipInfo(name);info.create_system=3;info.compress_type=zipfile.ZIP_DEFLATED
            info.external_attr=(stat.S_IFREG | (0o755 if not windows and name.startswith("amp-") else 0o644))<<16
            archive.writestr(info,path.read_bytes())
    return names

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--source",type=Path,required=True);p.add_argument("--output",type=Path,required=True);a=p.parse_args();package(a.source,a.output)
