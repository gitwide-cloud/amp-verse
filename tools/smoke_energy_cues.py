"""Real decoded-audio energy cue regression; generated fixtures only."""
import json, math, os, struct, subprocess, sys, tempfile, wave
from pathlib import Path
binary=str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as tmp:
    root=Path(tmp)/'music';root.mkdir()
    env=dict(os.environ,AMP_DEVICE_CONFIG_DIR=str(Path(tmp)/'device'))
    for name,opening in [('drop',False),('opening',True)]:
        with wave.open(str(root/(name+'.wav')),'wb') as audio:
            audio.setparams((1,2,11025,0,'NONE','not compressed'))
            pcm=bytearray()
            for i in range(11025*24):
                t=i/11025
                loud=(6<=t<17) or (opening and t<3)
                amplitude=0.7 if loud else 0.035
                value=amplitude*math.sin(2*math.pi*220*t)
                if 1.5<=t<1.51:value=0.95*math.sin(2*math.pi*220*t)
                pcm.extend(struct.pack('<h',int(value*32767)))
            audio.writeframes(pcm)
    subprocess.run([binary,str(root),'--jobs','2'],env=env,check=True)
    records=root/'.ampverse'/'tracks'
    drop=json.loads((records/'drop.wav.json').read_text())
    opening=json.loads((records/'opening.wav.json').read_text())
    assert abs(drop['cue_in']-6)<0.3,drop
    assert opening['cue_in']<0.2,opening
    assert any(c['kind']=='drop' for c in drop['cues'])
    assert any(c['kind']=='energy-start' for c in opening['cues'])
    assert any(c['id']=='auto-in' and c['seconds']<0.1 for c in drop['cues'])
    assert any(c['id']=='auto-out' for c in drop['cues'])
    assert any(c['kind']=='mix-in' and c['seconds']==drop['cue_in'] for c in drop['cues'])
    drop['manual_override']=True;drop['cue_in']=2
    drop['manual_cues']=[{'id':'mine','label':'My cue','seconds':3,'kind':'hotcue','number':1}]
    path=records/'drop.wav.json';path.write_text(json.dumps(drop))
    subprocess.run([binary,str(root),'--force'],env=env,check=True)
    kept=json.loads(path.read_text())
    assert kept['cue_in']==2 and kept['manual_cues']==drop['manual_cues'],kept
    assert abs(kept['analysis_recommendation']['cue_in']-6)<0.3
print('PASS: decoded high-energy drop/opening, spike rejection, boundaries and protected manual proposal')
