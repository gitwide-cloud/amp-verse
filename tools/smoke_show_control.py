"""HTTP/WS regression against the actual desktop process; generates its own silent music."""
import base64, json, os, socket, struct, subprocess, sys, tempfile, time, urllib.request, urllib.error, uuid, wave
from pathlib import Path
binary = str(Path(sys.argv[1]).resolve())
def http(base, path, body=None, password="test-secret"):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(base+path, data=data, headers={"Content-Type":"application/json", "x-amp-password":password})
    try:
        with urllib.request.urlopen(request, timeout=8) as response: return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        raw=error.read().decode()
        try: raw=json.loads(raw)
        except ValueError: pass
        return error.code, raw
def receive(sock):
    def exact(n):
        result=b""
        while len(result)<n:
            chunk=sock.recv(n-len(result))
            if not chunk: raise RuntimeError("WebSocket closed before feedback")
            result+=chunk
        return result
    header=exact(2); length=header[1]&127
    if length==126: length=struct.unpack("!H",exact(2))[0]
    elif length==127: length=struct.unpack("!Q",exact(8))[0]
    assert not header[1]&128
    payload=exact(length)
    assert header[0]&15==1, header
    return json.loads(payload)
def websocket(port):
    sock=socket.create_connection(("127.0.0.1",port),timeout=3)
    key=base64.b64encode(os.urandom(16)).decode()
    sock.sendall((f"GET /api/v1/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\nx-amp-password: test-secret\r\n\r\n").encode())
    header=b""
    while not header.endswith(b"\r\n\r\n"): header+=sock.recv(1)
    assert b"101 Switching Protocols" in header, header
    return sock
with tempfile.TemporaryDirectory() as folder:
    root=Path(folder)
    with wave.open(str(root/"Silence.wav"),"wb") as audio:
        audio.setnchannels(1);audio.setsampwidth(2);audio.setframerate(8000);audio.writeframes(bytes(8000*2))
    port_socket=socket.socket();port_socket.bind(("127.0.0.1",0));port=port_socket.getsockname()[1];port_socket.close()
    base=f"http://127.0.0.1:{port}"
    env=os.environ.copy();env.update(AMP_CONTROL_BIND=f"127.0.0.1:{port}",AMP_CONTROL_PASSWORD="test-secret",AMP_VERSE_CONFIG_DIR=str(root/"settings"))
    for name in ("AMP_CONTROL_DISABLE","AMP_SCREENSHOT","AMP_SCREENSHOT_VIEW","AMP_DOCK_SMOKE"):env.pop(name,None)
    process=subprocess.Popen([binary,str(root)],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW if os.name=="nt" else 0)
    try:
        until=time.monotonic()+20
        while True:
            try:
                status,value=http(base,"/api/v1/catalog")
                if status==200 and len(value["songs"])==1:break
            except (OSError,ValueError): pass
            assert process.poll() is None, "desktop exited"
            assert time.monotonic()<until, "show control did not become ready"
            time.sleep(.05)
        assert len(value["playlists"])==1
        assert value["playlists"][0]["name"]=="Demo"
        assert http(base,"/api/v1/state",password="wrong")[0]==401
        def command(action, **fields):
            body=dict(id=str(uuid.uuid4()),action=action,**fields);return body,http(base,"/api/v1/command",body)
        one,(status,value)=command("set_energy",target=7)
        assert status==200 and value["ok"],value
        _,(status,value)=command("set_energy",target=5);assert status==200,value
        assert http(base,"/api/v1/command",one)[0]==200
        changed=dict(one,target=6);assert http(base,"/api/v1/command",changed)[0]==409
        command("manual_lock",enabled=True)
        _,(status,value)=command("set_energy",target=3);assert status==409 and not value["ok"]
        until=time.monotonic()+3
        while True:
            feedback=http(base,"/api/v1/state")[1]["amp"]
            if feedback["manual_dj_locked"] and feedback["target_energy"]==5:break
            assert time.monotonic()<until,feedback
            time.sleep(.02)
        for attempt in range(2):
            sock=websocket(port)
            try:
                state=receive(sock)
                assert state["amp"]["target_energy"]==5, state
                assert state["amp"]["playback_state"]=="stopped",state
                assert state["amp"]["manual_dj_locked"] is True,state
            finally: sock.close()
        state=http(base,"/api/v1/state")[1]
        assert state["amp"]["target_energy"]==5 and state["amp"]["connected"]
        print("PASS: native desktop HTTP, authenticated feedback, retries/conflicts, manual lock and passive WebSocket reconnect")
    finally:
        process.terminate()
        try:process.wait(timeout=5)
        except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
