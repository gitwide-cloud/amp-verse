"""Real six-deck desktop HTTP regression; isolated generated silent fixtures only."""
import json, os, socket, subprocess, sys, tempfile, time, urllib.request, urllib.error, uuid, wave
from pathlib import Path
binary=str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="amp-six-") as folder:
 root=Path(folder); music=root/"music";music.mkdir();(music/".ampverse").mkdir()
 for number in range(6):
  with wave.open(str(music/f"Track {number+1}.wav"),"wb") as audio:
   audio.setnchannels(2);audio.setsampwidth(2);audio.setframerate(8000);audio.writeframes(bytes(8000*4*30))
 (music/".ampverse/show.json").write_text(json.dumps({"version":1,"playlists":[{"id":"demo","name":"Demo","style":"steady","companion_available":True}]}))
 port_socket=socket.socket();port_socket.bind(("127.0.0.1",0));port=port_socket.getsockname()[1];port_socket.close();base=f"http://127.0.0.1:{port}"
 env=os.environ.copy();env.update(AMP_CONTROL_BIND=f"127.0.0.1:{port}",AMP_VERSE_CONFIG_DIR=str(root/"settings"))
 for key in ("AMP_CONTROL_DISABLE","AMP_SCREENSHOT","AMP_SCREENSHOT_VIEW"):env.pop(key,None)
 process=subprocess.Popen([binary,str(music)],env=env,creationflags=subprocess.CREATE_NO_WINDOW if os.name=="nt" else 0)
 def read(path,body=None):
  request=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers={"Content-Type":"application/json"})
  try:
   with urllib.request.urlopen(request,timeout=8) as response:return response.status,json.load(response)
  except urllib.error.HTTPError as error:return error.code,error.read().decode()
 def command(action,**fields):
  status,value=read("/api/v1/command",dict(id=str(uuid.uuid4()),action=action,**fields));assert status==200,(action,status,value);return value
 def state():return read("/api/v1/state")[1]
 def wait(predicate):
  until=time.monotonic()+10
  while True:
   current=state()
   if predicate(current):return current
   assert time.monotonic()<until,current
   time.sleep(.05)
 try:
  until=time.monotonic()+25
  while True:
   try:
    code,catalog=read("/api/v1/catalog")
    if code==200 and len(catalog["songs"])==6:break
   except (OSError,ValueError):pass
   assert process.poll() is None
   assert time.monotonic()<until,"Desktop did not initialize"
   time.sleep(.1)
  command("adjust_volume",delta=-1)
  names=["a1","b1","a2","b2","a3","b3"]
  for name,song in zip(names,catalog["songs"]):command("call_song",deck=name,song=song["id"],execution="load_only")
  wait(lambda s:all(d["ready"] for d in s["deck_layers"]))
  for name in names:command("play",deck=name)
  before=wait(lambda s:all(d["playing"] for d in s["deck_layers"]))
  command("select_live",deck="a3");command("select_live",deck="b2")
  selected=wait(lambda s:s["selected_decks"]==["A3","B2"])
  assert selected["crossfade_pair"]==["A1","B1"]
  status,_=read("/api/v1/command",dict(id=str(uuid.uuid4()),action="assign_crossfade_pair",a="a3",b="b2"));assert status==409
  command("save_music_scene",name="Test scene")
  scene_until=time.monotonic()+5
  while True:
   scenes=read("/api/v1/resources/scenes")[1]["scenes"]
   if scenes:break
   assert time.monotonic()<scene_until
   time.sleep(.1)
  command("recall_music_scene",scene=scenes[0]["id"],execution="load_only")
  assert all(d["playing"] for d in state()["deck_layers"])
  command("set_crossfader",value=1);command("fade",deck="a2",target=0,seconds=.1,stop_after=True)
  after=wait(lambda s:not s["deck_layers"][2]["playing"])
  assert all(after["deck_layers"][d]["playing"] for d in [0,1,3,4,5])
  assert all(after["deck_layers"][d]["position"]>=before["deck_layers"][d]["position"] for d in [0,1,3,4,5])
  page=read("/api/v1/resources?playlist=demo&limit=2")[1];assert len(page["songs"])==2 and page["total"]==6,page
  assert read("/api/v1/resources?playlist=hidden")[0]==400
  command("stop_all_decks");wait(lambda s:not any(d["playing"] for d in s["deck_layers"]))
  if len(sys.argv)>2:
   adb=sys.argv[2];subprocess.run([adb,"reverse",f"tcp:{port}",f"tcp:{port}"],check=True)
   try:
    result=subprocess.run([adb,"shell","am","instrument","-w","-e","class","cloud.gitwide.ampverse.RemoteDeviceTest","-e","desktop_url",base,"cloud.gitwide.ampverse.test/androidx.test.runner.AndroidJUnitRunner"],check=True,timeout=50,capture_output=True,text=True)
    print(result.stdout);assert "OK (" in result.stdout and "FAILURES" not in result.stdout,result.stdout
   finally:subprocess.run([adb,"reverse","--remove",f"tcp:{port}"])
  command("stop_all_decks");wait(lambda s:not any(d["playing"] for d in s["deck_layers"]))
  print("PASS: six actual decoders, transport, focus/routing isolation, independent fade, scoped resources, emergency stop")
 finally:
  process.terminate()
  try:process.wait(timeout=5)
  except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
