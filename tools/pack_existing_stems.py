"""Offline migration of this player's prepared-stems cache; source music is never touched."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sqlite3


def pack(root):
    root = root.resolve(strict=True)
    db = sqlite3.connect(root / "stems.ampdb", timeout=30)
    db.execute("PRAGMA journal_mode=DELETE")
    db.execute("PRAGMA synchronous=FULL")
    db.execute("CREATE TABLE IF NOT EXISTS records(key TEXT PRIMARY KEY,document TEXT NOT NULL)")
    db.execute("CREATE TABLE IF NOT EXISTS stem_audio(stem TEXT NOT NULL,name TEXT NOT NULL,audio BLOB NOT NULL,UNIQUE(stem,name))")
    completed = 0
    try:
        for folder in sorted(root.iterdir()):
            if not folder.is_dir() or folder.is_symlink() or not re.fullmatch(r"[a-f0-9]{64}", folder.name):
                continue
            source = folder / "manifest.json"
            if not source.is_file():
                continue
            manifest = json.loads(source.read_text(encoding="utf-8"))
            if manifest.get("source_stamp") != folder.name:
                raise ValueError(f"Cache identity mismatch: {folder.name}")
            parts = []
            for name in ("vocals", "instrumental", "drums"):
                audio = folder / f"{name}.wav"
                if audio.is_symlink() or not audio.resolve(strict=True).is_relative_to(root):
                    raise ValueError("Stem leaves cache")
                with audio.open("rb") as stream:
                    checksum = hashlib.file_digest(stream, "sha256").hexdigest()
                expected = manifest["audio"]["files"][name]
                if checksum != expected["sha256"] or audio.stat().st_size != expected["bytes"]:
                    raise ValueError(f"Cached stem failed validation: {folder.name}/{name}")
                parts.append((name, audio, checksum))
            with db:
                for name, audio, _ in parts:
                    cursor = db.execute("INSERT OR REPLACE INTO stem_audio(stem,name,audio) VALUES(?,?,zeroblob(?))", (folder.name, name, audio.stat().st_size))
                    with db.blobopen("stem_audio", "audio", cursor.lastrowid) as blob, audio.open("rb") as stream:
                        while chunk := stream.read(65536):
                            blob.write(chunk)
                db.execute("INSERT OR REPLACE INTO records VALUES(?,?)", (folder.name, json.dumps(manifest)))
                for metadata in folder.glob("*.json"):
                    if metadata.is_symlink():
                        raise ValueError("Metadata link refused")
                    value = json.loads(metadata.read_text(encoding="utf-8"))
                    db.execute("INSERT OR REPLACE INTO records VALUES(?,?)", (f"recovery/{folder.name}/{metadata.name}", json.dumps(value)))
            for name, _, checksum in parts:
                row = db.execute("SELECT rowid FROM stem_audio WHERE stem=? AND name=?", (folder.name, name)).fetchone()[0]
                digest = hashlib.sha256()
                with db.blobopen("stem_audio", "audio", row, readonly=True) as blob:
                    while chunk := blob.read(65536):
                        digest.update(chunk)
                if digest.hexdigest() != checksum:
                    raise ValueError("Archived audio did not verify; JSON retained")
            for metadata in folder.glob("*.json"):
                key = f"recovery/{folder.name}/{metadata.name}"
                copied = json.loads(db.execute("SELECT document FROM records WHERE key=?", (key,)).fetchone()[0])
                if copied != json.loads(metadata.read_text(encoding="utf-8")):
                    raise ValueError("Metadata changed; JSON retained")
                metadata.unlink()
            completed += 1
            print(f"Packed and verified {completed} cached songs", flush=True)
        if db.execute("PRAGMA quick_check").fetchone()[0] != "ok":
            raise ValueError("Archive integrity check failed")
    finally:
        db.close()
    return completed


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cache", type=Path)
    args = parser.parse_args()
    print(f"Completed: {pack(args.cache)} songs; original audio and playback WAVs retained")
