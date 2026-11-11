package cloud.gitwide.ampverse;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.*;import org.junit.runner.RunWith;import org.json.*;import java.util.*;import java.util.concurrent.*;import static org.junit.Assert.*;
/** Runs against isolated real Windows processes over the phone's LAN, never adb reverse. */
@RunWith(AndroidJUnit4.class)
public class LanCoreDeviceTest {
    String desktop,station,secret="test-secret";
    @Before public void setup(){desktop=InstrumentationRegistry.getArguments().getString("desktop_url");station=InstrumentationRegistry.getArguments().getString("cast_url");Assume.assumeNotNull(desktop,station);}
    JSONObject command(String session,String action,Object... fields)throws Exception{JSONObject body=new JSONObject().put("id",UUID.randomUUID().toString()).put("action",action);for(int i=0;i<fields.length;i+=2)body.put(fields[i].toString(),fields[i+1]);JSONObject result=RemotePanel.request(desktop,secret,"/api/v1/command",body,session);assertTrue(result.toString(),result.getBoolean("ok"));return result;}
    JSONObject state()throws Exception{return RemotePanel.request(desktop,secret,"/api/v1/state",null);}
    void waitFor(java.util.function.Predicate<JSONObject> predicate)throws Exception {long until=System.currentTimeMillis()+15000;JSONObject state;do{state=state();if(predicate.test(state))return;Thread.sleep(100);}while(System.currentTimeMillis()<until);fail("State not reached: "+state);}
    @Test public void discoveryOverActualWifi()throws Exception {
        boolean control=false,radio=false;
        for(ControlDiscovery.Server host:ControlDiscovery.find())if(host.url.equals(desktop))control=true;
        for(CastDiscovery.Server host:CastDiscovery.find())if(host.url.equals(station))radio=true;
        java.net.DatagramSocket udp=new java.net.DatagramSocket();udp.setSoTimeout(3000);byte[] q=ControlDiscovery.QUERY;udp.send(new java.net.DatagramPacket(q,q.length,java.net.InetAddress.getByName(new java.net.URI(desktop).getHost()),ControlDiscovery.PORT));
        byte[] packet=new byte[2048];java.net.DatagramPacket reply=new java.net.DatagramPacket(packet,packet.length);udp.receive(reply);System.out.println("Unicast desktop discovery="+new String(packet,0,reply.getLength(),java.nio.charset.StandardCharsets.UTF_8));udp.close();
        assertTrue("Broadcast desktop discovery",control);assertTrue("Broadcast Cast discovery",radio);
    }
    @Test public void desktopDiscoveryAuthenticationCommandsAndLiveEvents()throws Exception {
        assertTrue("LAN URL required",!desktop.contains("127.0.0.1")&&!desktop.contains("localhost"));
        java.util.List<ControlDiscovery.Server> hosts=ControlDiscovery.find();boolean discovered=false;for(ControlDiscovery.Server host:hosts)if(host.url.equals(desktop))discovered=true;System.out.println("Desktop discovery hosts="+hosts.size()+" matched="+discovered);
        try{RemotePanel.request(desktop,"wrong","/api/v1/info",null);fail("Wrong password accepted");}catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("401"));}
        JSONObject info=RemotePanel.request(desktop,secret,"/api/v1/info",null);assertEquals(1,info.getInt("protocol"));String session=info.getString("session");
        CountDownLatch updates=new CountDownLatch(2);java.util.concurrent.atomic.AtomicReference<String> error=new java.util.concurrent.atomic.AtomicReference<>();
        okhttp3.OkHttpClient client=new okhttp3.OkHttpClient.Builder().build();
        okhttp3.WebSocket socket=client.newWebSocket(new okhttp3.Request.Builder().url(desktop.replace("http://","ws://")+"/api/v1/events").header("x-amp-password",secret).header("x-amp-session",session).build(),new okhttp3.WebSocketListener(){@Override public void onMessage(okhttp3.WebSocket ws,String text){try{assertEquals(session,new JSONObject(text).getJSONObject("control").getString("session"));updates.countDown();}catch(Throwable e){error.set(e.toString());}}@Override public void onFailure(okhttp3.WebSocket ws,Throwable t,okhttp3.Response r){error.set(t.toString());}});
        JSONObject catalog=RemotePanel.request(desktop,secret,"/api/v1/catalog",null);assertTrue(catalog.getJSONArray("songs").length()>=2);String song=catalog.getJSONArray("songs").getJSONObject(0).getString("id");
        command(session,"call_song","song",song,"deck","a","execution","play_now");waitFor(s->s.optJSONArray("decks").optJSONObject(0).optBoolean("playing"));
        command(session,"adjust_volume","delta",-0.1);command(session,"pause","deck","a");command(session,"play","deck","a");command(session,"seek_relative","deck","a","seconds",1);
        String playlist=catalog.getJSONArray("playlists").getJSONObject(0).getString("id");
        command(session,"load_smart_playlist","playlist",playlist,"execution","load_only");command(session,"auto_dj","enabled",true);command(session,"set_style","style","hype");command(session,"set_energy","target",7.);waitFor(s->!s.optJSONObject("amp").isNull("current_energy"));command(session,"progression","target",6.,"transitions",4,"hold",true);
        command(session,"request_song","song",catalog.getJSONArray("songs").getJSONObject(1).getString("id"),"within",2);
        waitFor(s->s.optJSONObject("amp").optBoolean("auto_dj_enabled"));assertTrue("WebSocket state updates",updates.await(10,TimeUnit.SECONDS));assertNull(error.get());
        socket.close(1000,"Remote disconnect test");Thread.sleep(300);assertTrue("Remote disconnect must not stop host",state().getJSONObject("amp").getBoolean("auto_dj_enabled"));
        try{RemotePanel.request(desktop,secret,"/api/v1/command",new JSONObject().put("id",UUID.randomUUID().toString()).put("action","stop"),"outdated-session");fail("Stale session accepted");}catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("401"));}
        assertEquals(session,RemotePanel.request(desktop,secret,"/api/v1/info",null).getString("session"));
        command(session,"auto_dj","enabled",false);command(session,"stop","deck","a");command(session,"stop","deck","b");client.dispatcher().executorService().shutdown();client.connectionPool().evictAll();
    }
    @Test public void stationDiscoverySmartRadioAndActualPhoneAudio()throws Exception {
        boolean found=false;for(CastDiscovery.Server server:CastDiscovery.find())if(server.url.equals(station))found=true;System.out.println("Cast discovery matched="+found);
        CastClient client=new CastClient(station,secret);client.connect();JSONArray lists=client.get("/api/station/playlists").getJSONArray("playlists");assertTrue(lists.length()>0);
        client.post("/api/station/play/"+android.net.Uri.encode(lists.getJSONObject(0).getString("id")),null);
        long until=System.currentTimeMillis()+10000;while(client.status().isNull("now_playing")&&System.currentTimeMillis()<until)Thread.sleep(100);
        assertEquals("smart_radio",client.status().getString("source"));assertTrue(client.status().getBoolean("running"));
        final android.media.MediaPlayer[] player={null};CountDownLatch prepared=new CountDownLatch(1);java.util.concurrent.atomic.AtomicReference<String> error=new java.util.concurrent.atomic.AtomicReference<>();
        InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{try{player[0]=new android.media.MediaPlayer();player[0].setVolume(0,0);player[0].setOnPreparedListener(p->{p.start();prepared.countDown();});player[0].setOnErrorListener((p,a,b)->{error.set(a+"/"+b);prepared.countDown();return true;});player[0].setDataSource(client.stream());player[0].prepareAsync();}catch(Exception e){error.set(e.toString());prepared.countDown();}});
        try{assertTrue("Phone receives station stream",prepared.await(15,TimeUnit.SECONDS));assertNull(error.get());Thread.sleep(2500);final boolean[] active={false};InstrumentationRegistry.getInstrumentation().runOnMainSync(()->active[0]=player[0].isPlaying());assertTrue("Actual Android audio renderer playing",active[0]);assertTrue("Listener did not stop station",client.status().getBoolean("running"));}finally{InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{if(player[0]!=null)player[0].release();});}
    }
    @Test public void remoteScreenReconnectsAfterBackgroundWithoutStoppingPhonePlayback()throws Exception {
        android.content.Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        android.content.SharedPreferences prefs=context.getSharedPreferences("settings",0);String savedAddress=prefs.getString("remote-server",null);
        final androidx.media3.session.MediaController[] controller={null};
        final com.google.common.util.concurrent.ListenableFuture<androidx.media3.session.MediaController>[] future=new com.google.common.util.concurrent.ListenableFuture[1];
        InstrumentationRegistry.getInstrumentation().runOnMainSync(()->future[0]=new androidx.media3.session.MediaController.Builder(context,new androidx.media3.session.SessionToken(context,new android.content.ComponentName(context,PlaybackService.class))).buildAsync());
        controller[0]=future[0].get(10,TimeUnit.SECONDS);
        final java.util.List<androidx.media3.common.MediaItem> savedItems=new ArrayList<>();final int[] index={0};final long[] position={0};final float[] volume={1};final boolean[] playing={false};
        InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{for(int i=0;i<controller[0].getMediaItemCount();i++)savedItems.add(controller[0].getMediaItemAt(i));index[0]=Math.max(0,controller[0].getCurrentMediaItemIndex());position[0]=controller[0].getCurrentPosition();volume[0]=controller[0].getVolume();playing[0]=controller[0].getPlayWhenReady();controller[0].setVolume(0);controller[0].setMediaItem(androidx.media3.common.MediaItem.fromUri(station+"/listen.mp3"));controller[0].prepare();controller[0].play();});
        prefs.edit().putString("remote-server",desktop).commit();
        androidx.test.uiautomator.UiDevice device=androidx.test.uiautomator.UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
        device.wakeUp();
        try(androidx.test.core.app.ActivityScenario<MainActivity> scenario=androidx.test.core.app.ActivityScenario.launch(new android.content.Intent(context,MainActivity.class).putExtra("module","AMP Remote").addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))) {
            assertTrue(device.wait(androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.desc("Desktop control password")),10000));
            device.findObject(androidx.test.uiautomator.By.desc("Desktop control password")).setText(secret);
            device.findObject(androidx.test.uiautomator.By.text("Connect / Refresh")).click();
            assertTrue("Remote screen must authenticate and receive live state",device.wait(androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.text("Connected · live state updates")),15000));
            final boolean[] active={false};InstrumentationRegistry.getInstrumentation().runOnMainSync(()->active[0]=controller[0].isPlaying());assertTrue("Entering Remote did not pause phone audio",active[0]);
            scenario.moveToState(androidx.lifecycle.Lifecycle.State.CREATED);Thread.sleep(500);scenario.moveToState(androidx.lifecycle.Lifecycle.State.RESUMED);
            assertTrue("Returning from background restores authenticated state",device.wait(androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.text("Connected · live state updates")),15000));
            InstrumentationRegistry.getInstrumentation().runOnMainSync(()->active[0]=controller[0].isPlaying());assertTrue("Background reconnect did not stop phone audio",active[0]);
        }finally{
            android.content.SharedPreferences.Editor edit=prefs.edit();if(savedAddress==null)edit.remove("remote-server");else edit.putString("remote-server",savedAddress);edit.commit();
            InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{controller[0].setMediaItems(savedItems,index[0],position[0]);if(!savedItems.isEmpty())controller[0].prepare();controller[0].setVolume(volume[0]);controller[0].setPlayWhenReady(playing[0]);controller[0].release();});
        }
    }

    @Test public void phonePlaysActualLiveDesktopMaster()throws Exception {
        JSONObject info=RemotePanel.request(desktop,secret,"/api/v1/info",null);String session=info.getString("session");
        JSONArray songs=RemotePanel.request(desktop,secret,"/api/v1/catalog",null).getJSONArray("songs");assertEquals(3,songs.length());
        command(session,"auto_dj","enabled",false);command(session,"stop","deck","a");command(session,"stop","deck","b");
        command(session,"call_song","song",songs.getJSONObject(0).getString("id"),"deck","a","execution","play_now");waitFor(state->state.optJSONArray("decks").optJSONObject(0).optBoolean("playing"));command(session,"set_crossfader","value",0.);
        CastClient client=new CastClient(station,secret);client.connect();client.post("/api/live/launch",new JSONObject().put("source_password",secret));
        long until=System.currentTimeMillis()+10000;while(System.currentTimeMillis()<until){JSONObject state=client.status();if(state.optBoolean("running")&&state.optString("source").equals("live_dj"))break;Thread.sleep(100);}
        assertEquals("live_dj",client.status().getString("source"));assertTrue(client.status().getBoolean("running"));
        final android.media.MediaPlayer[] player={null};CountDownLatch ready=new CountDownLatch(1);java.util.concurrent.atomic.AtomicReference<String> error=new java.util.concurrent.atomic.AtomicReference<>();
        InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{try{player[0]=new android.media.MediaPlayer();player[0].setVolume(0,0);player[0].setOnPreparedListener(p->{p.start();ready.countDown();});player[0].setOnErrorListener((p,a,b)->{error.set(a+"/"+b);ready.countDown();return true;});player[0].setDataSource(client.stream());player[0].prepareAsync();}catch(Exception e){error.set(e.toString());ready.countDown();}});
        try{assertTrue(ready.await(15,TimeUnit.SECONDS));assertNull(error.get());Thread.sleep(1200);final boolean[] active={false};InstrumentationRegistry.getInstrumentation().runOnMainSync(()->active[0]=player[0].isPlaying());assertTrue("Phone audio renderer receives existing desktop master",active[0]);assertTrue("Desktop playback continues while phone listens",state().getJSONArray("decks").getJSONObject(0).getBoolean("playing"));}
        finally{InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{if(player[0]!=null)player[0].release();});command(session,"stop_live_broadcast");command(session,"stop","deck","a");client.post("/api/station/play/smart_demo",null);}
    }

}
