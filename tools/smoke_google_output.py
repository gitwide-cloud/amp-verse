"""Explicitly selected physical receiver test. Quiet generated audio; no personal library."""
import json,math,os,socket,struct,subprocess,sys,tempfile,time,urllib.request,wave
from pathlib import Path
player,cast,receiver_name=map(str,sys.argv[1:4]);player=str(Path(player).resolve());cast=str(Path(cast).resolve())
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def http(base,path,body=None):
 req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json'})
 with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req,timeout=6) as response:
  data=response.read();return json.loads(data) if data else None
with tempfile.TemporaryDirectory(prefix='amp-google-') as temp:
 root=Path(temp);music=root/'music';music.mkdir();pp=port();sp=port();base=f'http://127.0.0.1:{pp}';station=f'http://127.0.0.1:{sp}'
 with wave.open(str(music/'Quiet receiver test.wav'),'wb') as audio:
  audio.setparams((1,2,8000,0,'NONE','not compressed'));audio.writeframes(b''.join(struct.pack('<h',int(3*math.sin(i*2*math.pi*440/8000))) for i in range(8000*30)))
 env=os.environ.copy();server_env=dict(env,AMP_CAST_BIND=f'0.0.0.0:{sp}',AMP_CAST_CONFIG_DIR=str(root/'server'),AMP_ANALYSIS_ENABLED='0');player_env=dict(env,AMP_CONTROL_BIND=f'127.0.0.1:{pp}',AMP_VERSE_CONFIG_DIR=str(root/'settings'))
 flags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0;servers=[subprocess.Popen([cast,str(music)],env=server_env,creationflags=flags,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL),subprocess.Popen([player,str(music)],env=player_env,creationflags=flags,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)]
 def wait(read,predicate,seconds=15):
  until=time.monotonic()+seconds;last=None
  while time.monotonic()<until:
   try:
    last=read()
    if predicate(last):return last
   except (OSError,ValueError):pass
   time.sleep(.15)
  raise AssertionError(last)
 def state():return http(base,'/api/v1/state')['google_cast']
 def command(operation,**values):
  result=http(base,'/api/v1/command',dict(id=os.urandom(16).hex(),action='google_cast',operation=dict(operation=operation,**values)));assert result['ok'],result
 try:
  wait(lambda:http(station,'/api/status'),lambda s:s is not None);wait(lambda:http(base,'/api/v1/info'),lambda s:s['protocol']==1)
  http(station,'/api/start',{});wait(lambda:http(station,'/api/status'),lambda s:s['on_air'])
  command('discover');found=wait(state,lambda s:not s['discovering'] and len(s['receivers'])>0)
  matches=[d for d in found['receivers'] if d['name'].casefold()==receiver_name.casefold()];assert len(matches)==1,{'selected':receiver_name,'available':[d['name'] for d in found['receivers']]}
  selected=matches[0];command('start',receiver=selected['id'],address=f'127.0.0.1:{sp}')
  playing=wait(state,lambda s:s['playback_state']=='PLAYING',30);assert playing['receiver']['id']==selected['id']
  volume=playing.get('volume')
  if volume and volume[0] is not None and volume[1] is not None:
   command('volume',level=volume[0],muted=volume[1]);wait(state,lambda s:s.get('volume')==volume);print('PASS: receiver volume acknowledged without changing its existing level/mute')
  command('stop');wait(state,lambda s:not s['stopping'] and s['playback_state'] is None)
  print('PASS: selected receiver confirmed PLAYING; targeted stop confirmed; station/player left independent')
 finally:
  try:command('stop')
  except Exception:pass
  try:http(station,'/api/stop',{})
  except Exception:pass
  for process in servers:
   process.terminate()
   try:process.wait(timeout=5)
   except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
