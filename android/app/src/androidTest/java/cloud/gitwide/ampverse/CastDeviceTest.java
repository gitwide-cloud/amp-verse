package cloud.gitwide.ampverse;

import android.content.*;
import androidx.media3.common.MediaItem;
import androidx.media3.session.*;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.uiautomator.*;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.util.*;
import java.util.concurrent.TimeUnit;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public class CastDeviceTest {
    private void main(Runnable action){InstrumentationRegistry.getInstrumentation().runOnMainSync(action);}
    private void showText(String text)throws Exception {new UiScrollable(new UiSelector().scrollable(true)).scrollIntoView(new UiSelector().text(text));}
    private void showDescription(String text)throws Exception {new UiScrollable(new UiSelector().scrollable(true)).scrollIntoView(new UiSelector().description(text));}
    @Test public void controlsSelectedServerAndListensThroughPhoneService()throws Exception {
        String url=InstrumentationRegistry.getArguments().getString("castUrl");
        org.junit.Assume.assumeNotNull(url); // Requires the isolated generated-audio fixture server.
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        android.content.SharedPreferences prefs=context.getSharedPreferences("settings",Context.MODE_PRIVATE);
        String savedServer=prefs.getString("cast-server",null);
        final MediaController[] player=new MediaController[1];
        final List<MediaItem> saved=new ArrayList<>();final float[] volume={1};final long[] position={0};final int[] index={0};final boolean[] playing={false};
        UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
        CastClient client=new CastClient(url,"test-secret");client.connect();
        CastClient rejected=new CastClient(url,"incorrect");rejected.connect();
        try{rejected.command(true);fail("Bad password must not start station");}catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("rejected"));}
        assertFalse(client.status().getBoolean("running"));assertTrue(client.playlist().getJSONArray("tracks").length()>0);
        try {
            prefs.edit().putString("cast-server",url).commit();
            com.google.common.util.concurrent.ListenableFuture<MediaController>[] future=new com.google.common.util.concurrent.ListenableFuture[1];
            main(()->future[0]=new MediaController.Builder(context,new SessionToken(context,new ComponentName(context,PlaybackService.class))).buildAsync());
            player[0]=future[0].get(10,TimeUnit.SECONDS);
            main(()->{for(int i=0;i<player[0].getMediaItemCount();i++)saved.add(player[0].getMediaItemAt(i));volume[0]=player[0].getVolume();position[0]=player[0].getCurrentPosition();index[0]=Math.max(0,player[0].getCurrentMediaItemIndex());playing[0]=player[0].getPlayWhenReady();player[0].setVolume(0);});
            Intent intent=new Intent(context,MainActivity.class).putExtra("module","AMP Cast").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(intent)) {
                assertTrue(device.wait(Until.hasObject(By.text("Advanced · server address and password")),5000));
                device.findObject(By.text("Advanced · server address and password")).click();
                assertTrue(device.wait(Until.hasObject(By.text("Connect")),5000));
                device.findObject(By.desc("AMP Cast admin password")).setText("incorrect");
                device.findObject(By.text("Connect")).click();
                assertTrue(device.wait(Until.hasObject(By.textContains("control rejected")),12000));
                Thread.sleep(5500);
                assertTrue("Polling must preserve auth rejection",device.hasObject(By.textContains("control rejected")));
                device.findObject(By.desc("AMP Cast admin password")).setText("test-secret");
                device.findObject(By.text("Connect")).click();
                assertTrue(device.wait(Until.hasObject(By.text("Server connected and access verified. Choose an output; existing station playback is unchanged.")),12000));
                showDescription("Advanced · station downloads and diagnostics");if(device.findObject(By.desc("Advanced · station downloads and diagnostics")).getText().endsWith("+"))device.findObject(By.desc("Advanced · station downloads and diagnostics")).click();
                showDescription("Search station songs");device.findObject(By.desc("Search station songs")).setText("zzzz-no-match");
                assertTrue(device.wait(Until.hasObject(By.text("No songs match your search.")),3000));
                showDescription("Search station songs");device.findObject(By.desc("Search station songs")).setText("phone-test");
                assertTrue(device.wait(Until.hasObject(By.textContains("Phone-test")),3000));
                showDescription("Search station songs");device.findObject(By.desc("Search station songs")).setText("");
                scenario.onActivity(a->((android.view.inputmethod.InputMethodManager)a.getSystemService(Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(a.getWindow().getDecorView().getWindowToken(),0));
                showDescription("Source actions · change host program");if(device.findObject(By.desc("Source actions · change host program")).getText().endsWith("+"))device.findObject(By.desc("Source actions · change host program")).click();
                showText("Start host station");device.findObject(By.text("Start host station")).click();
                showText("Station started");assertTrue(device.wait(Until.hasObject(By.text("Station started")),12000));assertTrue(client.status().getBoolean("running"));
                showText("Listen on this phone");assertTrue(device.wait(Until.hasObject(By.text("Listen on this phone").enabled(true)),12000));
                device.findObject(By.text("Listen on this phone")).click();
                long until=System.currentTimeMillis()+12000;boolean[] active={false};
                while(System.currentTimeMillis()<until){main(()->active[0]=player[0].isPlaying());if(active[0])break;Thread.sleep(100);}
                assertTrue("Real server MP3 must play through phone service",active[0]);
                showText("Stop phone listening");device.findObject(By.text("Stop phone listening")).click();assertTrue(client.status().getBoolean("running"));
                showText("Disconnect from server");device.findObject(By.text("Disconnect from server")).click();
                assertTrue(device.wait(Until.hasObject(By.textContains("Server disconnected.")),5000));
                assertTrue("Disconnect must preserve station",client.status().getBoolean("running"));
                showText("Advanced · server address and password");device.findObject(By.text("Advanced · server address and password")).click();
                showText("Connect");device.findObject(By.text("Connect")).click();
                assertTrue(device.wait(Until.hasObject(By.text("Server connected and access verified. Choose an output; existing station playback is unchanged.")),12000));
                showText("Stop host program for all listeners");device.findObject(By.text("Stop host program for all listeners")).click();showText("Station stopped");assertTrue(device.wait(Until.hasObject(By.text("Station stopped")),12000));assertFalse(client.status().getBoolean("running"));
            }
        }finally {
            client.command(false);
            if(player[0]!=null)main(()->{player[0].stop();if(saved.isEmpty())player[0].clearMediaItems();else player[0].setMediaItems(saved,index[0],position[0]);player[0].setVolume(volume[0]);if(playing[0]&&!saved.isEmpty()){player[0].prepare();player[0].play();}player[0].release();});
            if(savedServer==null)prefs.edit().remove("cast-server").commit();else prefs.edit().putString("cast-server",savedServer).commit();
        }
    }
    @Test public void createPlaylistPersistsOnFixtureWithoutChangingStation()throws Exception {
        String url=InstrumentationRegistry.getArguments().getString("castUrl");
        org.junit.Assume.assumeNotNull(url); // Isolated fixture only; never OFFICECOMP.
        CastClient client=new CastClient(url,"test-secret");
        org.json.JSONObject before=client.connect();
        org.json.JSONArray tracks=client.get("/api/studio/tracks?limit=1").getJSONArray("tracks");
        assertTrue(tracks.length()>0);
        String name="Android fixture "+java.util.UUID.randomUUID();
        org.json.JSONObject saved=client.postJson("/api/studio/playlists",new org.json.JSONObject().put("name",name).put("tracks",new org.json.JSONArray().put(tracks.getJSONObject(0).getInt("id"))));
        CastClient reopened=new CastClient(url,"test-secret");reopened.connect();
        org.json.JSONArray lists=reopened.get("/api/studio/playlists").getJSONArray("playlists");
        org.json.JSONObject found=null;
        for(int i=0;i<lists.length();i++)if(saved.getString("id").equals(lists.getJSONObject(i).optString("id")))found=lists.getJSONObject(i);
        assertNotNull("New client must read saved server playlist",found);assertEquals(name,found.getString("name"));assertEquals(1,found.getJSONArray("tracks").length());
        org.json.JSONObject after=reopened.status();assertEquals(before.getString("station_id"),after.getString("station_id"));assertEquals(before.getBoolean("running"),after.getBoolean("running"));assertEquals(before.optString("current_source"),after.optString("current_source"));
    }
    @Test public void userEntryRejectsLocalButMockClientRemainsUsable()throws Exception {
        for(String value:new String[]{"127.0.0.1:8787","localhost:8787","http://localhost.:8787","http://[::1]:8787","http://[0:0:0:0:0:0:0:1]:8787","http://[::]:8787","0.0.0.0:8787"}) {
            try{CastClient.requireLanAddress(value);fail("LAN address required");}catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("LAN"));}
        }
        CastClient.requireLanAddress("192.168.1.50:8787");
        assertEquals("http://127.0.0.1:8787",new CastClient("127.0.0.1:8787","").base);
    }
    @Test public void rejectsAmbiguousServerAddresses()throws Exception {
        for(String value:new String[]{"file:///music","http://user:secret@localhost:8787","http://localhost:8787/api/status","http://localhost:8787?station=other"}){
            try{new CastClient(value,"");fail("Rejected address expected");}catch(Exception expected){assertTrue(expected.getMessage().contains("server address"));}
        }
    }
}
