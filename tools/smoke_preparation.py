"""Generated-audio preparation regression. No personal music is accessed."""
import json, math, os, struct, subprocess, sys, tempfile, wave, shutil, hashlib
from pathlib import Path
from fixture_catalog import read,write,raw
binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as tmp:
    folder = Path(tmp) / "music"; folder.mkdir()
    audio = folder / "C-major-120.wav"
    rate = 11025; frames = []
    for i in range(rate * 10):
        t = i / rate; value = 0.
        if 1 <= t < 9:
            value = sum(a * math.sin(2 * math.pi * f * t) for a,f in [(0.25,261.6256),(0.16,329.6276),(0.16,391.9954)])
            beat = (t - 1) % 0.5
            value += 0.3 * math.exp(-beat * 100) * math.sin(2 * math.pi * 1400 * t)
        frames.append(struct.pack("<h", int(max(-1,min(1,value)) * 32767)))
    with wave.open(str(audio), "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate); w.writeframes(b"".join(frames))
    copies = [folder / "parallel-a.wav", folder / "parallel-b.wav"]
    for other in copies: shutil.copyfile(audio, other)
    source_hash = hashlib.sha256(audio.read_bytes()).hexdigest()
    subprocess.run([binary, str(folder), "--jobs", "3"], check=True)
    assert hashlib.sha256(audio.read_bytes()).hexdigest() == source_hash
    for other in copies:
        parallel = read(folder,"tracks/"+other.name+".json")
        assert abs(parallel["bpm"] - 120) < 2 and parallel["harmonic_key"] == "8B", parallel
        assert hashlib.sha256(other.read_bytes()).hexdigest() == source_hash
    sidecar = "tracks/"+audio.name+".json"
    prep = read(folder,sidecar)
    assert abs(prep["bpm"] - 120) < 2, prep
    assert prep["harmonic_key"] == "8B", prep
    assert abs(next(c["seconds"] for c in prep["cues"] if c["id"] == "auto-in") - 1) < .03, prep
    assert .97 <= prep["cue_in"] < 1.5 and abs(prep["cue_out"] - 9) < .03, prep
    prep["manual_cues"] = [{"id":"my-drop", "label":"My drop", "seconds":3., "kind":"hotcue"}]
    prep["manual_override"] = True; prep["cue_in"] = 2.; prep["cue_out"] = 8.
    prep["vendor_extension"] = {"preserved":True}
    write(folder,sidecar,prep)
    subprocess.run([binary, str(folder), "--jobs", "3", "--force"], check=True)
    prep = read(folder,sidecar)
    assert prep["cue_in"] == 2 and prep["cue_out"] == 8
    assert prep["manual_cues"][0]["id"] == "my-drop" and prep["vendor_extension"]["preserved"]
    manual_audio = folder / "manual-first.wav"; shutil.copyfile(audio, manual_audio)
    manual_sidecar = "tracks/manual-first.wav.json"
    write(folder,manual_sidecar,{"cue_in":2.,"cue_out":8.,"bpm":127.,"manual_override":True})
    subprocess.run([binary,str(folder),"--jobs","3"],check=True)
    manual = read(folder,manual_sidecar)
    assert manual["bpm"] == 127 and manual["source_sha256"] == source_hash
    assert manual["analysis_recommendation"]["analysis"]["engine"] == "amp-prep-heuristic-5"
    del manual["analysis_recommendation"]; write(folder,manual_sidecar,manual)
    declined_bytes = raw(folder,manual_sidecar)
    subprocess.run([binary,str(folder),"--jobs","3"],check=True)
    assert raw(folder,manual_sidecar) == declined_bytes, "Declined proposals must not be repeatedly recomputed"
    moved = Path(tmp) / "relocated"; moved.mkdir()
    shutil.copyfile(audio, moved / audio.name); shutil.copytree(folder / "ampdj", moved / "ampdj")
    subprocess.run([binary, str(moved)], check=True)
    assert read(moved,sidecar)["source_sha256"] == prep["source_sha256"]
print("PASS: parallel song analysis, tempo/key, silence trim, manual cues, extensions and relocated metadata")
