"""Real isolated Rust server LAN-discovery protocol, no personal music."""
import json, os, socket, subprocess, sys, tempfile, time, urllib.request, wave
from pathlib import Path
binary=str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as tmp:
    root=Path(tmp);music=root/'music';music.mkdir()
    with wave.open(str(music/'fixture.wav'),'wb') as out:out.setparams((1,2,8000,0,'NONE','not compressed'));out.writeframes(b'\x00\x00'*8000)
    def free_port(kind):
        with socket.socket(type=kind) as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
    api=free_port(socket.SOCK_STREAM);port=free_port(socket.SOCK_DGRAM)
    env=dict(os.environ,AMP_CAST_BIND=f'0.0.0.0:{api}',AMP_CAST_DISCOVERY_PORT=str(port),AMP_CAST_PASSWORD='fixture-only',AMP_CAST_CONFIG_DIR=str(root/'server'),AMP_DEVICE_CONFIG_DIR=str(root/'device'),AMP_ANALYSIS_ENABLED='false')
    process=subprocess.Popen([binary,str(music)],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
    try:
        until=time.monotonic()+8
        while True:
            try:
                with urllib.request.urlopen(f'http://127.0.0.1:{api}/api/status',timeout=1) as response:status=json.load(response)
                break
            except OSError:
                if time.monotonic()>until:raise
                time.sleep(.05)
        with socket.socket(type=socket.SOCK_DGRAM) as s:
            s.settimeout(.3);s.sendto(b'invalid',('127.0.0.1',port))
            try:s.recvfrom(2048);raise AssertionError('Malformed discovery query answered')
            except socket.timeout:pass
            s.settimeout(2);s.sendto(b'AMPVERSE_CAST_DISCOVER_V1',('127.0.0.1',port));data,peer=s.recvfrom(2048);reply=json.loads(data)
            assert reply['service']=='amp-cast' and reply['protocol']==1 and reply['port']==api
            assert reply['server_id']==status['server_id'] and reply['station_id']==status['station_id']
            assert not status['running'] and not status['analysis']['enabled']
        print('PASS: real discovery response, identity/port binding, malformed-query rejection and no automatic broadcast/analysis')
    finally:
        process.terminate();process.wait(timeout=5)
