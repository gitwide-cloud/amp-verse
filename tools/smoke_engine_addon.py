"""Exercise the separately packaged engine using generated local audio only."""
import json, math, struct, subprocess, sys, tempfile, wave
from pathlib import Path
from fixture_catalog import raw
binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    with wave.open(str(root / 'tone.wav'), 'wb') as out:
        out.setparams((1, 2, 11025, 0, 'NONE', 'not compressed'))
        out.writeframes(b''.join(struct.pack('<h', int(12000*math.sin(i*2*math.pi*440/11025))) for i in range(33075)))
    def request(**extra):
        payload = dict(protocol=1, root=str(root), relative='tone.wav', draft=None)
        payload.update(extra)
        result = subprocess.run([binary], input=json.dumps(payload).encode(), capture_output=True, timeout=45, check=True)
        return json.loads(result.stdout)
    assert request(protocol=99)['status'] == 'error'
    assert not (root / '.ampverse').exists() and not (root / 'ampdj').exists(), 'Bad protocol must not start work'
    first = request()
    assert first['status'] == 'prepared', first
    key = "tracks/tone.wav.json"
    before = raw(root,key)
    assert request()['status'] == 'skipped'
    assert raw(root,key) == before
    assert request(relative='../outside.wav')['status'] == 'error'
    assert raw(root,key) == before
    draft = first['preparation']; draft['bpm'] = 127; draft['manual_override'] = True
    reviewed = request(draft=draft)
    assert reviewed['status'] == 'prepared' and reviewed['preparation']['bpm'] == 127
    assert raw(root,key) == before, 'Draft analysis must not publish canonical metadata'
print('PASS: optional process protocol, version rejection, coordinated analysis/reuse, path confinement and manual draft protection')
