package cloud.gitwide.ampverse;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;
import android.content.*;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import org.json.*;
@RunWith(AndroidJUnit4.class)
public class RadioOfflineTest {
 private static final class PlaybackSnapshot {
  final String owner,mediaId;final boolean playWhenReady;final int count;
  PlaybackSnapshot(){androidx.media3.common.Player player=PlaybackService.player();assertNotNull(player);owner=PlaybackService.owner();playWhenReady=player.getPlayWhenReady();count=player.getMediaItemCount();mediaId=player.getCurrentMediaItem()==null?null:player.getCurrentMediaItem().mediaId;}
  void assertUnchanged(){androidx.media3.common.Player player=PlaybackService.player();assertNotNull(player);assertEquals("Offline browsing must not change playback ownership",owner,PlaybackService.owner());assertEquals("Offline browsing must not start or stop playback",playWhenReady,player.getPlayWhenReady());assertEquals("Offline browsing must not replace the queue",count,player.getMediaItemCount());assertEquals(mediaId,player.getCurrentMediaItem()==null?null:player.getCurrentMediaItem().mediaId);}
 }
 @Test public void emptyOfflineListOffersRecoveryWithoutStartingPlayback()throws Exception {
  Context app=InstrumentationRegistry.getInstrumentation().getTargetContext();
  org.junit.Assume.assumeTrue(RadioOffline.albums(app).isEmpty());
  androidx.test.uiautomator.UiDevice device=androidx.test.uiautomator.UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
  device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
  Intent intent=new Intent(app,MainActivity.class).putExtra("module","AMP Cast").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
  try(androidx.test.core.app.ActivityScenario<MainActivity> scenario=androidx.test.core.app.ActivityScenario.launch(intent)) {
   java.util.concurrent.atomic.AtomicReference<PlaybackSnapshot> before=new java.util.concurrent.atomic.AtomicReference<>();
   for(int attempt=0;attempt<100&&before.get()==null;attempt++){
    scenario.onActivity(activity->{if(PlaybackService.player()!=null)before.set(new PlaybackSnapshot());});
    if(before.get()==null)android.os.SystemClock.sleep(50);
   }
   assertNotNull("Playback service must connect before checking ownership",before.get());
   // The primary card is labelled Offline. The older Offline playlists button
   // remains inside collapsed More; do not target that hidden legacy control.
   scenario.onActivity(activity->{android.view.View card=activity.getWindow().getDecorView().findViewWithTag("cast-offline-playlists");assertNotNull("Primary offline card must exist",card);assertTrue(card.isShown());assertTrue(card.isEnabled());card.requestRectangleOnScreen(new android.graphics.Rect(0,0,card.getWidth(),card.getHeight()),true);});
   androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().waitForIdleSync();
   androidx.test.uiautomator.UiObject2 offline=device.wait(androidx.test.uiautomator.Until.findObject(androidx.test.uiautomator.By.desc("Open offline playlists")),5000);
   assertNotNull("Primary offline card must be visible and accessible",offline);assertFalse(offline.getVisibleBounds().isEmpty());
   CardScreenshots.capture("card-offline-action.png");
   offline.click();
   assertTrue(device.wait(androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.text("No offline playlists yet")),3000));
   assertNotNull(device.findObject(androidx.test.uiautomator.By.text(java.util.regex.Pattern.compile("Find stations",java.util.regex.Pattern.CASE_INSENSITIVE))));
   assertNotNull(device.findObject(androidx.test.uiautomator.By.textContains("does not download its songs")));
   scenario.onActivity(activity->before.get().assertUnchanged());
   CardScreenshots.capture("card-offline-recovery.png");
   device.findObject(androidx.test.uiautomator.By.text(java.util.regex.Pattern.compile("Close",java.util.regex.Pattern.CASE_INSENSITIVE))).click();
   androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().waitForIdleSync();
   scenario.onActivity(activity->before.get().assertUnchanged());
  }
 }
 @Test public void cancelDisconnectsAStalledAudioTransfer()throws Exception {
  Context app=InstrumentationRegistry.getInstrumentation().getTargetContext();File output=File.createTempFile("offline-cancel-",".part",app.getCacheDir());
  ServerSocket server=new ServerSocket(0,1,CastTestAddress.privateAddress());java.util.concurrent.CountDownLatch headers=new java.util.concurrent.CountDownLatch(1),release=new java.util.concurrent.CountDownLatch(1);
  Thread host=new Thread(()->{try(Socket socket=server.accept()){BufferedReader reader=new BufferedReader(new InputStreamReader(socket.getInputStream()));String line;while((line=reader.readLine())!=null&&!line.isEmpty()){}socket.getOutputStream().write("HTTP/1.1 200 OK\r\nContent-Length: 10000\r\n\r\na".getBytes(StandardCharsets.US_ASCII));socket.getOutputStream().flush();headers.countDown();release.await(8,java.util.concurrent.TimeUnit.SECONDS);}catch(Exception ignored){}});host.start();
  java.util.concurrent.atomic.AtomicBoolean cancel=new java.util.concurrent.atomic.AtomicBoolean();java.util.concurrent.ExecutorService executor=java.util.concurrent.Executors.newSingleThreadExecutor();
  try{
   CastClient client=new CastClient("http://"+server.getInetAddress().getHostAddress()+":"+server.getLocalPort(),"");
   java.util.concurrent.Future<Boolean> task=executor.submit(()->{try{client.download(0,output,10000,"unused",cancel::get);return false;}catch(Exception expected){return true;}});
   assertTrue(headers.await(3,java.util.concurrent.TimeUnit.SECONDS));cancel.set(true);
   assertTrue("Cancel must not wait for the twenty-second network timeout",task.get(3,java.util.concurrent.TimeUnit.SECONDS));
  }finally{cancel.set(true);release.countDown();server.close();host.join(2000);executor.shutdownNow();output.delete();}
 }
 @Test public void originalAudioAndCuesSurviveOfflineDownloadAndConflict()throws Exception {
  Context app=InstrumentationRegistry.getInstrumentation().getTargetContext();
  File sandbox=new File(app.getCacheDir(),"station-test-"+java.util.UUID.randomUUID());assertTrue(sandbox.mkdir());
  Context isolated=new ContextWrapper(app){@Override public File getFilesDir(){return sandbox;}};
  assertTrue(RadioOffline.albums(isolated).isEmpty());
  File pending=new File(sandbox,"radio-offline/.pending-fixture/ampdj");assertTrue(pending.mkdirs());new File(pending,"catalog.ampdb").createNewFile();
  assertTrue("Incomplete downloads must not appear",RadioOffline.albums(isolated).isEmpty());
  new File(pending,"catalog.ampdb").delete();pending.delete();pending.getParentFile().delete();
  byte[] audio="original test audio".getBytes(StandardCharsets.UTF_8);
  StringBuilder checksum=new StringBuilder();for(byte b:java.security.MessageDigest.getInstance("SHA-256").digest(audio))checksum.append(String.format(java.util.Locale.ROOT,"%02x",b&255));
  JSONObject prep=new JSONObject().put("cue_in",1.25).put("cue_out",8.).put("bpm",128).put("manual_override",true).put("manual_cues",new JSONArray().put(new JSONObject().put("id","my-drop").put("number",3).put("seconds",1.25).put("label","My drop")));
  JSONObject metadata=new JSONObject().put("server_id","server").put("station_id","station").put("library_id","library").put("bytes",audio.length).put("sha256",checksum.toString()).put("preparation",prep);
  java.util.concurrent.atomic.AtomicInteger audioRequests=new java.util.concurrent.atomic.AtomicInteger(),prepRequests=new java.util.concurrent.atomic.AtomicInteger();
  ServerSocket server=new ServerSocket(0,8,CastTestAddress.privateAddress());
  Thread thread=new Thread(()->{while(!server.isClosed())try(Socket client=server.accept()){
   BufferedReader reader=new BufferedReader(new InputStreamReader(client.getInputStream(),StandardCharsets.US_ASCII));String request=reader.readLine(),line;while((line=reader.readLine())!=null&&!line.isEmpty()){}
   boolean basic=request.contains("/basic "),preparation=request.contains("/preparation ");
   byte[] body;
   if(basic){JSONObject value=new JSONObject(metadata.toString());value.remove("preparation");body=value.toString().getBytes(StandardCharsets.UTF_8);}
   else if(preparation||request.contains("/metadata ")){prepRequests.incrementAndGet();body=metadata.toString().getBytes(StandardCharsets.UTF_8);}
   else{audioRequests.incrementAndGet();body=audio;}
   OutputStream output=client.getOutputStream();output.write(("HTTP/1.1 200 OK\r\nContent-Length: "+body.length+"\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));output.write(body);output.flush();
  }catch(Exception error){if(!server.isClosed())throw new RuntimeException(error);}});thread.start();
  try {
   CastClient client=new CastClient("http://"+server.getInetAddress().getHostAddress()+":"+server.getLocalPort(),"");
   JSONObject manifest=new JSONObject().put("server_id","server").put("station_id","station").put("library_id","library").put("tracks",new JSONArray().put(new JSONObject().put("id",0).put("title","Test song").put("extension","wav")));
   JSONObject playlist=new JSONObject().put("name","My station playlist").put("tracks",new JSONArray().put(0).put(0));
   java.util.List<Integer> progress=new java.util.ArrayList<>();
   try{RadioOffline.save(isolated,client,manifest,new JSONObject().put("tracks",new JSONArray()));fail("Empty playlist must not publish");}catch(IOException expected){assertTrue(expected.getMessage().contains("no songs"));}
   File saved=RadioOffline.save(isolated,client,manifest,playlist,new RadioOffline.Progress(){public void song(int i,int n,String title){assertEquals(2,n);progress.add(i);}public boolean cancelled(){return false;}},false);
   assertEquals(java.util.Arrays.asList(1,2),progress);
   assertEquals(1,audioRequests.get());assertEquals(0,prepRequests.get());
   assertFalse(RadioOffline.album(saved).getJSONArray("items").getJSONObject(0).has("preparation"));
   androidx.media3.common.MediaItem basicSong=RadioOffline.items(saved).get(0);assertNull(basicSong.mediaMetadata.extras.getString("amp-preparation"));
   PhoneCues.save(isolated,basicSong,0,2700);
   RadioOffline.Progress uncancelled=new RadioOffline.Progress(){public void song(int i,int n,String title){}public boolean cancelled(){return false;}};
   assertEquals(saved,RadioOffline.save(isolated,client,manifest,playlist,uncancelled,true));
   assertEquals("Preparation catch-up must not download audio",1,audioRequests.get());assertEquals(1,prepRequests.get());
   assertEquals(basicSong.mediaId,RadioOffline.items(saved).get(0).mediaId);
   assertEquals(2700,PhoneCues.seconds(isolated,RadioOffline.items(saved).get(0),0));
   RadioOffline.save(isolated,client,manifest,playlist,uncancelled,false);
   assertEquals(1,prepRequests.get());assertEquals(1,audioRequests.get());
   assertTrue(RadioOffline.album(saved).getJSONArray("items").getJSONObject(0).has("remote_preparation"));

   java.util.concurrent.atomic.AtomicBoolean cancel=new java.util.concurrent.atomic.AtomicBoolean();
   try{RadioOffline.save(isolated,client,manifest,playlist,new RadioOffline.Progress(){public void song(int i,int n,String title){if(i==2)cancel.set(true);}public boolean cancelled(){return cancel.get();}});fail("Cancelled playlist must not publish");}catch(IOException expected){}
   assertEquals(1,RadioOffline.albums(isolated).size());
   assertEquals(1,new File(sandbox,"radio-offline").listFiles().length);

   android.net.Uri managed=RadioOffline.items(saved).get(0).localConfiguration.uri;
   try(InputStream in=isolated.getContentResolver().openInputStream(managed)){assertArrayEquals(audio,in.readAllBytes());}
   if(android.os.Build.VERSION.SDK_INT>=29){assertEquals("content",managed.getScheme());try(android.database.Cursor c=isolated.getContentResolver().query(managed,new String[]{android.provider.MediaStore.Audio.Media.RELATIVE_PATH},null,null,null)){assertTrue(c.moveToFirst());assertTrue(c.getString(0).startsWith(ManagedMusic.PATH));}}
   assertEquals(2,RadioOffline.items(saved).size());
   JSONObject cues=new JSONObject(RadioOffline.items(saved).get(0).mediaMetadata.extras.getString("amp-preparation"));
   assertEquals("my-drop",cues.getJSONArray("manual_cues").getJSONObject(0).getString("id"));
   assertEquals(3,cues.getJSONArray("manual_cues").getJSONObject(0).getInt("number"));
   assertEquals(0,LibraryStore.read(isolated).size());
   manifest.put("station_id","changed");try{RadioOffline.save(isolated,client,manifest,playlist);fail("Must reject changed station");}catch(IOException expected){}
   assertEquals(1,RadioOffline.albums(isolated).size());
   assertEquals(1,new File(sandbox,"radio-offline").listFiles().length);
  } finally {for(File folder:RadioOffline.albums(isolated))try{for(androidx.media3.common.MediaItem item:RadioOffline.items(folder))isolated.getSharedPreferences("phone-cues",0).edit().remove(item.mediaId+":0").remove(item.mediaId+":manual:0").commit();}catch(Exception ignored){}for(File folder:RadioOffline.albums(isolated))try{for(androidx.media3.common.MediaItem item:RadioOffline.items(folder))if("content".equals(item.localConfiguration.uri.getScheme()))ManagedMusic.discard(isolated,item.localConfiguration.uri);}catch(Exception ignored){}server.close();thread.join(2000);try(java.util.stream.Stream<Path> paths=Files.walk(sandbox.toPath())){paths.sorted(java.util.Comparator.reverseOrder()).forEach(path->path.toFile().delete());}}
 }
}
