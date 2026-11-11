"""Generated files only: two devices, incremental analysis and independent servers."""
import hashlib,json,math,os,socket,struct,subprocess,sys,tempfile,time,urllib.request,urllib.error,wave
from pathlib import Path
prepare=str(Path(sys.argv[1]).resolve());cast=str(Path(sys.argv[2]).resolve())
def port():
    with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def request(base,path,post=False,headers=None):
    req=urllib.request.Request(base+path,data=b'' if post else None,headers=headers or {})
    try:
        with urllib.request.urlopen(req,timeout=5) as r:
            b=r.read();return r.status,json.loads(b) if b else None
    except urllib.error.HTTPError as e:return e.code,None
with tempfile.TemporaryDirectory(prefix='amp-coordination-',dir=os.environ.get('AMP_COORD_TEST_PARENT')) as tmp:
    folder=Path(tmp);music=folder/'music';music.mkdir()
    for n in range(6):
        with wave.open(str(music/f'song{n}.wav'),'wb') as w:
            w.setparams((1,2,11025,0,'NONE','not compressed'))
            w.writeframes(b''.join(struct.pack('<h',int(6000*math.sin(2*math.pi*(220+n*20)*i/11025))) for i in range(11025)))
    envs=[]
    for n in range(2):
        env=dict(os.environ,AMP_DEVICE_CONFIG_DIR=str(folder/f'device{n}'),AMP_CAST_CONFIG_DIR=str(folder/f'server{n}'),AMP_CAST_BIND=f'127.0.0.1:{port()}',AMP_ANALYSIS_ENABLED='false')
        envs.append(env)
    jobs=[subprocess.Popen([prepare,str(music),'--jobs','2'],env=e,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) for e in envs]
    outputs=[]
    for p in jobs:
        out,err=p.communicate(timeout=45);assert p.returncode==0,(out,err);outputs.append(out)
    assert sum(s.count('/ BPM ') for s in outputs)==6,outputs
    records={p.name:p.read_bytes() for p in (music/'ampdj/tracks').glob('*.json')}
    assert len(records)==6
    repeat=subprocess.run([prepare,str(music),'--jobs','3'],env=envs[1],capture_output=True,text=True,timeout=30)
    assert repeat.returncode==0 and repeat.stdout.count('skipped Complete')==6,repeat.stdout+repeat.stderr
    assert records=={p.name:p.read_bytes() for p in (music/'ampdj/tracks').glob('*.json')}
    assert json.loads((folder/'device0/device.json').read_text())['id']!=json.loads((folder/'device1/device.json').read_text())['id']
    def start(env):
        p=subprocess.Popen([cast,str(music)],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        base='http://'+env['AMP_CAST_BIND'];until=time.monotonic()+10
        while time.monotonic()<until:
            try:return p,base,request(base,'/api/status')[1]
            except (OSError,ValueError):time.sleep(.05)
        p.terminate();raise AssertionError('Server failed to start')
    servers=[]
    try:
        for env in envs:servers.append(start(env))
        a,b=servers;sa,sb=a[2],b[2]
        assert sa['server_id']!=sb['server_id'] and sa['station_id']!=sb['station_id']
        assert sa['library_id']==sb['library_id']
        assert sa['owner_server_id']==sa['server_id'] and sb['owner_server_id']==sb['server_id']
        assert request(a[1],'/api/start',True,{'x-amp-server':sb['server_id']})[0]==401
        assert request(a[1],'/api/start',True)[0]==204
        assert request(a[1],'/api/status')[1]['running'] and not request(b[1],'/api/status')[1]['running']
        assert request(b[1],'/api/start',True)[0]==204
        assert request(a[1],'/api/stop',True)[0]==204
        assert request(b[1],'/api/status')[1]['running']
        assert request(a[1],'/api/analysis/resume',True)[0]==204
        assert json.loads((folder/'server0/server.json').read_text())['analysis_enabled'] is True
        a[0].terminate();a[0].wait(timeout=5)
        restart=start(envs[0]);servers[0]=restart
        assert restart[2]['server_id']==sa['server_id'] and restart[2]['station_id']==sa['station_id']
        assert restart[2]['analysis']['enabled'] is True
        until=time.monotonic()+10
        while True:
            state=request(restart[1],'/api/status')[1]['analysis']
            if state['reused_or_claimed']==6:break
            assert time.monotonic()<until,state
            time.sleep(.1)
        assert records=={p.name:p.read_bytes() for p in (music/'ampdj/tracks').glob('*.json')}
        print('PASS: two devices coordinate six jobs exactly once; reuse preserves bytes; server IDs persist; station controls stay independent; headless analysis resumes')
    finally:
        for p,base,_ in servers:
            if p.poll() is None:
                try:request(base,'/api/stop',True)
                except OSError:pass
        # Allow the encoder's OS handle cleanup to finish after async cancellation.
        time.sleep(.2)
        for p,_,_ in servers:
            if p.poll() is None:p.terminate()
        for p,_,_ in servers:
            try:p.wait(timeout=5)
            except subprocess.TimeoutExpired:p.kill();p.wait(timeout=5)
