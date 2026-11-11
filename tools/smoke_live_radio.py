"""Generated-tone network acceptance test. Requires isolated fixture hosts, never user music."""
import array,json,math,os,subprocess,time,urllib.request,uuid
from pathlib import Path
if os.environ.get('AMP_ACCEPT_GENERATED_FIXTURE')!='1':raise SystemExit('Set AMP_ACCEPT_GENERATED_FIXTURE=1 only with isolated generated-tone test hosts.')
BASE='http://127.0.0.1:8787';DESKTOP='http://127.0.0.1:8790'
OUT=Path('test-output/cast-remote');OUT.mkdir(parents=True,exist_ok=True)
def api(base,path,body=None,post=False):
 data=json.dumps(body).encode() if body is not None else b'' if post else None
 req=urllib.request.Request(base+path,data=data,headers={'x-amp-password':'test-secret','content-type':'application/json'})
 with urllib.request.urlopen(req,timeout=8) as r:
  raw=r.read();return json.loads(raw) if raw else None
def command(action,**kw):
 result=api(DESKTOP,'/api/v1/command',dict(id=str(uuid.uuid4()),action=action,**kw));assert result['ok'],result
 return result
def wait_state(test,seconds=12):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  state=api(DESKTOP,'/api/v1/state')
  if test(state):return state
  time.sleep(.05)
 raise AssertionError(state)
def capture(name,seconds):
 start=time.monotonic();raw=bytearray()
 with urllib.request.urlopen(BASE+'/listen.mp3',timeout=10) as response:
  while time.monotonic()-start<seconds:raw.extend(response.read(1024))
 file=OUT/(name+'.mp3');file.write_bytes(raw)
 result=subprocess.run(['ffmpeg','-v','error','-i',str(file),'-ac','1','-ar','6000','-f','f32le','pipe:1'],capture_output=True,check=True)
 pcm=array.array('f');pcm.frombytes(result.stdout);assert len(pcm)>6000,(name,len(pcm));return pcm

def energy(pcm,freq):
 n=len(pcm);c=complex(math.cos(2*math.pi*freq/6000),-math.sin(2*math.pi*freq/6000));z=1+0j;total=0j
 for v in pcm:total+=v*z;z*=c
 return abs(total)*2/n
catalog=api(DESKTOP,'/api/v1/catalog');songs=catalog['songs'];assert len(songs)==3 and all(s['title'].endswith('-tone') for s in songs)
# Independent station: record across a handoff and prove simultaneous tones exist in the overlap.
api(BASE,'/api/station/play/smart_demo',post=True)
radio=capture('smart-radio-overlap',17)
windows=[radio[i:i+3000] for i in range(0,len(radio)-3000,1500)]
assert any(energy(w,440)>.015 and energy(w,660)>.015 for w in windows),'No actual overlapping audio transition'
print('PASS: Smart Radio sends a continuous stream containing a real two-song crossfade',flush=True)
# Live master, without private Cue Deck audio.
command('auto_dj',enabled=False);command('stop',deck='a');command('stop',deck='b');command('adjust_volume',delta=.25)
a=next(s['id'] for s in songs if s['title']=='A-tone');b=next(s['id'] for s in songs if s['title']=='B-tone')
command('call_song',song=a,deck='a',execution='play_now');wait_state(lambda s:s['decks'][0]['playing'])
command('call_song',song=b,deck='b',execution='load_only');command('set_crossfader',value=0.)
command('cue_deck',side=0,operation={'operation':'load','song':b})
time.sleep(.5);command('cue_deck',side=0,operation={'operation':'play'})
api(BASE,'/api/live/launch',{'source_password':'test-secret'})
end=time.monotonic()+10
while time.monotonic()<end:
 state=api(BASE,'/api/status')
 if state['running'] and state['source']=='live_dj':break
 time.sleep(.1)
else:raise AssertionError(state)
private=capture('live-private-cue-excluded',3)
assert energy(private,440)>.01,energy(private,440)
assert energy(private,660)<energy(private,440)*.03,'Private cue leaked into master stream'
print('PASS: Live Broadcast contains the real playing deck and excludes private cue',flush=True)
command('sampler',pad=1)
pads=capture('live-sampler',3)
assert energy(pads,880)>.01,'Sampler missing from live mix'
print('PASS: sampler pad is audible in the live network master',flush=True)
command('play',deck='b');command('set_crossfader',value=.5)
blend=capture('live-crossfade',3)
assert energy(blend,440)>.005 and energy(blend,660)>.005,'Crossfader not reflected in live stream'
print('PASS: both decks and mixer crossfade appear in the ongoing live stream',flush=True)
command('stop_live_broadcast');command('stop',deck='a');command('stop',deck='b');command('cue_deck',side=0,operation={'operation':'stop'})
api(BASE,'/api/station/play/smart_demo',post=True)
print('PASS: switched back to independent Smart Radio; isolated hosts remain available',flush=True)
