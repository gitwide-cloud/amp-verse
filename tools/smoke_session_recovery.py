"""Crash/restart recovery against a real desktop host using isolated silent audio.

Usage: python tools/smoke_session_recovery.py path/to/amp-player.exe
The fixture enables the optional restore-paused policy in its own local database.
No user library/settings are modified and no playback is requested.
"""
import contextlib, json, os, socket, sqlite3, subprocess, sys, tempfile, time, urllib.request, uuid, wave
from pathlib import Path
binary=Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='amp-recovery-') as directory:
    root=Path(directory);music=root/'music';music.mkdir();config=root/'settings'
    with wave.open(str(music/'Recovery.wav'),'wb') as audio:
        audio.setnchannels(1);audio.setsampwidth(2);audio.setframerate(8000);audio.writeframes(bytes(8000*2*20))
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
    env=os.environ.copy();env.update(AMP_VERSE_CONFIG_DIR=str(config),AMP_CONTROL_BIND=f'127.0.0.1:{port}',AMP_CONTROL_PASSWORD='recovery-test')
    for key in ('AMP_SCREENSHOT','AMP_SCREENSHOT_VIEW','AMP_DOCK_SMOKE','AMP_CONTROL_DISABLE'):env.pop(key,None)
    def launch():
        return subprocess.Popen([str(binary),str(music)],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0)
    def http(path,body=None):
        data=None if body is None else json.dumps(body).encode()
        request=urllib.request.Request(f'http://127.0.0.1:{port}'+path,data=data,headers={'Content-Type':'application/json','x-amp-password':'recovery-test'})
        with urllib.request.urlopen(request,timeout=5) as response:return json.load(response)
    def wait_for(check):
        end=time.monotonic()+25
        while time.monotonic()<end:
            try:
                value=check()
                if value:return value
            except (OSError,ValueError,sqlite3.Error):pass
            time.sleep(.1)
        raise AssertionError('Recovery condition timed out')
    def command(action,**fields):
        result=http('/api/v1/command',dict(id=str(uuid.uuid4()),action=action,**fields));assert result['ok'],result
    process=launch()
    try:
        catalog=wait_for(lambda: (c if len(c['songs'])==1 else None) if (c:=http('/api/v1/catalog')) else None)
        command('call_song',song=catalog['songs'][0]['id'],deck='a',execution='load_only')
        command('seek_relative',deck='a',seconds=4)
        file=config/'ampdj/session.ampdb'
        def checkpoint():
            if not file.exists():return None
            with contextlib.closing(sqlite3.connect(file)) as db:rows=db.execute('select key,document from records').fetchall()
            for key,document in rows:
                saved=json.loads(document)
                if saved['decks'][0] and saved['decks'][0]['position']>=4:return key,saved
        key,saved=wait_for(checkpoint)
        process.kill();process.wait(timeout=5)
        # Model the user's checked restore-paused preference, only in this fixture.
        saved['automatic_restore']=True;saved['was_auto']=True
        with contextlib.closing(sqlite3.connect(file)) as db:db.execute('update records set document=? where key=?',(json.dumps(saved),key));db.commit()
        process=launch()
        state=wait_for(lambda: (s if s['decks'][0]['loaded'] else None) if (s:=http('/api/v1/state')) else None)
        assert not any(deck['playing'] for deck in state['decks']),state
        assert not state['amp']['auto_dj_enabled'],state
        assert abs(state['decks'][0]['position']-saved['decks'][0]['position'])<.1,state
        print('PASS: real crash checkpoint, automatic paused restoration, source position retained, Auto DJ and audio remain stopped')
    finally:
        if process.poll() is None:process.kill();process.wait(timeout=5)

