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
    private void dialogButton(UiDevice device,String text)throws Exception {BySelector button=By.text(java.util.regex.Pattern.compile(java.util.regex.Pattern.quote(text),java.util.regex.Pattern.CASE_INSENSITIVE));assertTrue(device.wait(Until.hasObject(button),5000));device.findObject(button).click();}
    private void hideKeyboard(UiDevice device)throws Exception {main(()->{for(android.app.Activity activity:androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry.getInstance().getActivitiesInStage(androidx.test.runner.lifecycle.Stage.RESUMED))((android.view.inputmethod.InputMethodManager)activity.getSystemService(Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(activity.getWindow().getDecorView().getWindowToken(),0);});}
    private void top()throws Exception {new UiScrollable(new UiSelector().scrollable(true)).scrollToBeginning(30);}
    private void expand(UiDevice device,String title)throws Exception {showDescription(title);UiObject2 toggle=device.findObject(By.desc(title));if(toggle.getText().endsWith("+"))toggle.click();}
    @Test public void controlsSelectedServerAndListensThroughPhoneService()throws Exception {
        String url=InstrumentationRegistry.getArguments().getString("castUrl");
        org.junit.Assume.assumeNotNull(url); // Requires the isolated generated-audio fixture server.
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        android.content.SharedPreferences prefs=context.getSharedPreferences("settings",Context.MODE_PRIVATE);
        String savedServer=prefs.getString("cast-server",null);
        android.content.SharedPreferences sections=context.getSharedPreferences("ui-sections",0);Map<String,?> savedSections=new HashMap<>(sections.getAll());sections.edit().clear().commit();
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
                expand(device,"Advanced");showText("Advanced · server address and password");
                device.findObject(By.text("Advanced · server address and password")).click();
                showText("Connect");assertTrue(device.wait(Until.hasObject(By.text("Connect")),5000));
                device.findObject(By.desc("AMP Cast admin password")).setText("incorrect");
                hideKeyboard(device);showText("Connect");device.findObject(By.text("Connect")).click();top();
                assertTrue(device.wait(Until.hasObject(By.textContains("control rejected")),12000));
                Thread.sleep(5500);
                assertTrue("Polling must preserve auth rejection",device.hasObject(By.textContains("control rejected")));
                showDescription("AMP Cast admin password");device.findObject(By.desc("AMP Cast admin password")).setText("test-secret");
                hideKeyboard(device);showText("Connect");device.findObject(By.text("Connect")).click();top();
                assertTrue(device.wait(Until.hasObject(By.text("Server connected and access verified. Choose an output; existing station playback is unchanged.")),12000));
                expand(device,"Choose songs for a new playlist");
                new UiScrollable(new UiSelector().scrollable(true)).scrollIntoView(new UiSelector().className("android.widget.CheckBox"));
                device.findObject(By.clazz("android.widget.CheckBox")).click();
                showText("Create playlist on server (1 songs)");device.findObject(By.text("Create playlist on server (1 songs)")).click();
                String createdName="UI fixture "+UUID.randomUUID();
                device.findObject(By.desc("New server playlist name")).setText(createdName);dialogButton(device,"Save");top();
                assertTrue(device.wait(Until.hasObject(By.text("Playlist saved and read back from server; station unchanged.")),12000));
                assertFalse("Saving must not start station",client.status().getBoolean("running"));
                showText("Browse playlists");device.findObject(By.text("Browse playlists")).click();
                assertTrue(device.wait(Until.hasObject(By.text(createdName)),12000));device.findObject(By.text(createdName)).click();
                showText("Play selected playlist");device.findObject(By.text("Play selected playlist")).click();dialogButton(device,"Play");top();
                assertTrue(device.wait(Until.hasObject(By.text("Playlist requested on server. Phone listening is a separate output.")),12000));
                assertTrue(client.status().getBoolean("running"));
                showText("Listen on this phone");assertTrue(device.wait(Until.hasObject(By.text("Listen on this phone").enabled(true)),12000));
                device.findObject(By.text("Listen on this phone")).click();
                long until=System.currentTimeMillis()+12000;boolean[] active={false};
                while(System.currentTimeMillis()<until){main(()->active[0]=player[0].isPlaying());if(active[0])break;Thread.sleep(100);}
                assertTrue("Real server MP3 must play through phone service",active[0]);
                showText("Disconnect from server");device.findObject(By.text("Disconnect from server")).click();top();
                assertTrue(device.wait(Until.hasObject(By.textContains("Server disconnected.")),5000));
                assertTrue("Disconnect must preserve station",client.status().getBoolean("running"));
                showText("Stop phone listening");device.findObject(By.text("Stop phone listening")).click();
                long stoppedBy=System.currentTimeMillis()+5000;boolean[] stillPlaying={true};
                while(System.currentTimeMillis()<stoppedBy){main(()->stillPlaying[0]=player[0].isPlaying());if(!stillPlaying[0])break;Thread.sleep(100);}
                assertFalse("Phone stop after disconnect must stop only phone audio",stillPlaying[0]);assertTrue(client.status().getBoolean("running"));
                main(()->{player[0].setMediaItem(new MediaItem.Builder().setMediaId("fixture-non-radio").setUri(client.stream()).build());player[0].prepare();player[0].play();});
                long otherBy=System.currentTimeMillis()+10000;boolean[] otherActive={false};
                while(System.currentTimeMillis()<otherBy){main(()->otherActive[0]=player[0].isPlaying());if(otherActive[0])break;Thread.sleep(100);}
                assertTrue(otherActive[0]);ModuleRpc.call(context,"player_stop_radio",null);
                main(()->otherActive[0]=player[0].isPlaying());assertTrue("Standalone stop IPC must preserve non-radio playback",otherActive[0]);
                expand(device,"Advanced");
                showText("Advanced · server address and password");device.findObject(By.text("Advanced · server address and password")).click();
                showText("Connect");hideKeyboard(device);showText("Connect");device.findObject(By.text("Connect")).click();top();
                assertTrue(device.wait(Until.hasObject(By.text("Server connected and access verified. Choose an output; existing station playback is unchanged.")),12000));
                showText("Stop station playback");device.findObject(By.text("Stop station playback")).click();dialogButton(device,"Stop station");top();assertTrue(device.wait(Until.hasObject(By.text("Station stopped")),12000));assertFalse(client.status().getBoolean("running"));
            }
        }finally {
            android.content.SharedPreferences.Editor restoreSections=sections.edit().clear();for(Map.Entry<String,?> entry:savedSections.entrySet())if(entry.getValue() instanceof Boolean)restoreSections.putBoolean(entry.getKey(),(Boolean)entry.getValue());restoreSections.commit();
            client.command(false);
            if(player[0]!=null)main(()->{player[0].stop();if(saved.isEmpty())player[0].clearMediaItems();else player[0].setMediaItems(saved,index[0],position[0]);player[0].setVolume(volume[0]);if(playing[0]&&!saved.isEmpty()){player[0].prepare();player[0].play();}player[0].release();});
            if(savedServer==null)prefs.edit().remove("cast-server").commit();else prefs.edit().putString("cast-server",savedServer).commit();
        }
    }
    @Test public void namedFixtureOpensLibraryWithoutChangingStation()throws Exception {
        org.junit.Assume.assumeTrue("true".equals(InstrumentationRegistry.getArguments().getString("castDiscovery")));
        String url=InstrumentationRegistry.getArguments().getString("castUrl");org.junit.Assume.assumeNotNull(url);
        String password=InstrumentationRegistry.getArguments().getString("castPassword","test-secret");
        CastClient client=new CastClient(url,password);org.json.JSONObject before=client.connect();
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        android.content.SharedPreferences prefs=context.getSharedPreferences("settings",0);String previous=prefs.getString("cast-server",null);
        UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class).putExtra("module","AMP Cast").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))){
            assertTrue(device.wait(Until.hasObject(By.text("Find nearby servers")),5000));device.findObject(By.text("Find nearby servers")).click();
            String name=before.getString("station_name");assertTrue(device.wait(Until.hasObject(By.text(name)),12000));device.findObject(By.text(name)).click();
            if(before.optBoolean("password_required")){
                assertTrue(device.wait(Until.hasObject(By.desc("Selected server password")),12000));device.findObject(By.desc("Selected server password")).setText("incorrect");dialogButton(device,"Open Library");top();
                assertTrue(device.wait(Until.hasObject(By.textContains("control rejected")),12000));
                showText(name);device.findObject(By.text(name)).click();assertTrue(device.wait(Until.hasObject(By.desc("Selected server password")),12000));device.findObject(By.desc("Selected server password")).setText(password);dialogButton(device,"Open Library");
            }
            top();assertTrue(device.wait(Until.hasObject(By.text("Server connected and access verified. Choose an output; existing station playback is unchanged.")),12000));
            showText(name+" · Library");assertTrue(device.hasObject(By.text(name+" · Library")));
            assertFalse(device.hasObject(By.desc("Selected server password")));
            org.json.JSONObject after=client.status();assertEquals(before.getString("station_id"),after.getString("station_id"));assertEquals(before.getBoolean("running"),after.getBoolean("running"));assertEquals(before.optString("current_source"),after.optString("current_source"));
        }finally{if(previous==null)prefs.edit().remove("cast-server").commit();else prefs.edit().putString("cast-server",previous).commit();}
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
