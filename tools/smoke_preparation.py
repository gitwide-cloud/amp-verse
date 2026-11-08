"""Generated-audio preparation regression. No personal music is accessed."""
import json, math, os, struct, subprocess, sys, tempfile, wave, shutil, hashlib
from pathlib import Path
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
        parallel = json.loads((folder / ".ampverse" / "tracks" / (other.name + ".json")).read_text())
        assert abs(parallel["bpm"] - 120) < 2 and parallel["harmonic_key"] == "8B", parallel
        assert hashlib.sha256(other.read_bytes()).hexdigest() == source_hash
    sidecar = folder / ".ampverse" / "tracks" / (audio.name + ".json")
    prep = json.loads(sidecar.read_text())
    assert abs(prep["bpm"] - 120) < 2, prep
    assert prep["harmonic_key"] == "8B", prep
    assert abs(prep["cue_in"] - 1) < .03 and abs(prep["cue_out"] - 9) < .03, prep
    prep["manual_cues"] = [{"id":"my-drop", "label":"My drop", "seconds":3., "kind":"hotcue"}]
    prep["manual_override"] = True; prep["cue_in"] = 2.; prep["cue_out"] = 8.
    prep["vendor_extension"] = {"preserved":True}
    sidecar.write_text(json.dumps(prep))
    subprocess.run([binary, str(folder), "--jobs", "3", "--force"], check=True)
    prep = json.loads(sidecar.read_text())
    assert prep["cue_in"] == 2 and prep["cue_out"] == 8
    assert prep["manual_cues"][0]["id"] == "my-drop" and prep["vendor_extension"]["preserved"]
    manual_audio = folder / "manual-first.wav"; shutil.copyfile(audio, manual_audio)
    manual_sidecar = folder / ".ampverse" / "tracks" / "manual-first.wav.json"
    manual_sidecar.write_text(json.dumps({"cue_in":2.,"cue_out":8.,"bpm":127.,"manual_override":True}))
    subprocess.run([binary,str(folder),"--jobs","3"],check=True)
    manual = json.loads(manual_sidecar.read_text())
    assert manual["bpm"] == 127 and manual["source_sha256"] == source_hash
    assert manual["analysis_recommendation"]["analysis"]["engine"] == "amp-prep-heuristic-1"
    del manual["analysis_recommendation"]; manual_sidecar.write_text(json.dumps(manual))
    declined_bytes = manual_sidecar.read_bytes()
    subprocess.run([binary,str(folder),"--jobs","3"],check=True)
    assert manual_sidecar.read_bytes() == declined_bytes, "Declined proposals must not be repeatedly recomputed"
    moved = Path(tmp) / "relocated"; moved.mkdir()
    shutil.copyfile(audio, moved / audio.name); shutil.copytree(folder / ".ampverse", moved / ".ampverse")
    subprocess.run([binary, str(moved)], check=True)
    assert json.loads((moved / ".ampverse" / "tracks" / sidecar.name).read_text())["source_sha256"] == prep["source_sha256"]
print("PASS: parallel song analysis, tempo/key, silence trim, manual cues, extensions and relocated metadata")
