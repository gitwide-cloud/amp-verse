"""Real six-deck desktop HTTP regression; isolated generated silent fixtures only."""
import json, os, socket, subprocess, sys, tempfile, time, urllib.request, urllib.error, uuid, wave
from pathlib import Path
binary=str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="amp-six-") as folder:
 root=Path(folder); music=root/"music";music.mkdir();(music/".ampverse").mkdir()
 for number in range(6):
  with wave.open(str(music/f"Track {number+1}.wav"),"wb") as audio:
   audio.setnchannels(2);audio.setsampwidth(2);audio.setframerate(8000);audio.writeframes(bytes(8000*4*30))
  (music/f"Track {number+1}.wav.amp.json").write_text(json.dumps({"cue_in":0,"bpm":120,"beat_offset":0,"harmonic_key":"8A","manual_override":True}))
 (music/".ampverse/show.json").write_text(json.dumps({"version":1,"playlists":[{"id":"demo","name":"Demo","style":"steady","companion_available":True}]}))
 port_socket=socket.socket();port_socket.bind(("127.0.0.1",0));port=port_socket.getsockname()[1];port_socket.close();base=f"http://127.0.0.1:{port}"
 env=os.environ.copy();env.update(AMP_CONTROL_BIND=f"{os.environ.get('AMP_SMOKE_BIND','127.0.0.1')}:{port}",AMP_VERSE_CONFIG_DIR=str(root/"settings"))
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
   assert time.monotonic()<until, ("Desktop did not initialize", locals().get('code'), locals().get('catalog'), state())
   time.sleep(.1)
  command("adjust_volume",delta=-1)
  names=["a1","b1","a2","b2","a3","b3"]
  for name,song in zip(names,catalog["songs"]):command("call_song",deck=name,song=song["id"],execution="load_only")
  wait(lambda s:all(d["ready"] for d in s["deck_layers"]))
  feedback=None
  if os.environ.get("AMP_PROFILE")=="1":
   sys.path.insert(0,str(Path("test-output/bench-tools").resolve()));import psutil
   feedback=subprocess.Popen(["node","integrations/companion/tools/feedback-load.js",base],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
   def profile(label):
    parent=psutil.Process(process.pid);seen={};peak=0;start=time.monotonic()
    while time.monotonic()-start<3:
     group=[parent]+parent.children(recursive=True);rss=0
     for item in group:
      try:
       cpu=item.cpu_times();seconds=cpu.user+cpu.system;initial=seen.get(item.pid,(seconds,seconds))[0];seen[item.pid]=(initial,seconds);rss+=item.memory_info().rss
      except (psutil.NoSuchProcess,psutil.AccessDenied):pass
     peak=max(peak,rss);state();time.sleep(.2)
    elapsed=time.monotonic()-start;cpu=sum(end-begin for begin,end in seen.values())/elapsed*100
    print(f"PROFILE {label}: {cpu:.1f}% of one CPU core; peak {peak/1024/1024:.1f} MiB for player + decoders; HTTP 5 Hz + Companion WebSocket")
   for name in names[:2]:command("play",deck=name)
   profile("two active / six prepared")
  for name in names:command("play",deck=name)
  if os.environ.get("AMP_PROFILE")=="1":profile("six active")
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
  command("load_smart_playlist",playlist="demo",execution="load_only")
  command("crossfade_duration",seconds=.5)
  command("auto_dj",enabled=True)
  wait(lambda s:s["amp"]["auto_dj_enabled"] and any(d["loaded"] and not d["playing"] for d in s["deck_layers"][:2]))
  command("mix_next",immediate=True)
  wait(lambda s:s["amp"]["auto_dj_enabled"] and s["amp"]["fade_state"]=="idle" and s["deck_layers"][0]["playing"] and not s["deck_layers"][1]["playing"])
  preserved=state();assert not preserved["deck_layers"][2]["playing"] and all(preserved["deck_layers"][d]["playing"] for d in [3,4,5])
  command("auto_dj_pause",paused=True)
  paused=wait(lambda s:s["amp"]["auto_dj_paused"] and not any(d["playing"] for d in s["deck_layers"][:2]))
  paused_position=paused["deck_layers"][0]["position"]
  command("auto_dj_pause",paused=False)
  resumed=wait(lambda s:s["amp"]["auto_dj_enabled"] and s["deck_layers"][0]["playing"])
  assert resumed["deck_layers"][0]["position"]>=paused_position
  assert all(resumed["deck_layers"][d]["playing"] for d in [3,4,5])
  print("Actual decoder underrun counters:", [d["underruns"] for d in resumed["deck_layers"]])
  command("auto_dj",enabled=False)
  if feedback is not None:
   output=feedback.communicate(timeout=15)[0];print(output);assert feedback.returncode==0,output
  command("stop_all_decks");wait(lambda s:not any(d["playing"] for d in s["deck_layers"]))
  if len(sys.argv)>2:
   adb=sys.argv[2];android_host=os.environ.get("AMP_ANDROID_HOST","127.0.0.1")
   if android_host=="127.0.0.1":subprocess.run([adb,"reverse",f"tcp:{port}",f"tcp:{port}"],check=True)
   try:
    result=subprocess.run([adb,"shell","am","instrument","-w","-e","class","cloud.gitwide.ampverse.RemoteDeviceTest","-e","desktop_url",f"http://{android_host}:{port}","cloud.gitwide.ampverse.test/androidx.test.runner.AndroidJUnitRunner"],check=True,timeout=50,capture_output=True,text=True)
    print(result.stdout);assert "OK (" in result.stdout and "FAILURES" not in result.stdout,result.stdout
   finally:
    if android_host=="127.0.0.1":subprocess.run([adb,"reverse","--remove",f"tcp:{port}"])
  command("stop_all_decks");wait(lambda s:not any(d["playing"] for d in s["deck_layers"]))
  print("PASS: six actual decoders, focus/routing isolation, independent fade, Auto DJ handoff/pause/resume with background layers, scoped resources, emergency stop")
 finally:
  process.terminate()
  try:process.wait(timeout=5)
  except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
