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
    private UiObject2 text(UiDevice device,String value)throws Exception {
        new UiScrollable(new UiSelector().scrollable(true)).scrollIntoView(new UiSelector().text(value));
        UiObject2 found=device.wait(Until.findObject(By.text(value)),5000);assertNotNull(value,found);return found;
    }
    private UiObject2 description(UiDevice device,String value)throws Exception {
        new UiScrollable(new UiSelector().scrollable(true)).scrollIntoView(new UiSelector().description(value));
        UiObject2 found=device.wait(Until.findObject(By.desc(value)),5000);assertNotNull(value,found);return found;
    }
    private void awaitOnAir(CastClient client,boolean expected)throws Exception {
        long deadline=System.currentTimeMillis()+15000;
        while(System.currentTimeMillis()<deadline){org.json.JSONObject state=client.status();if(state.optBoolean("on_air")==expected&&!state.optBoolean("preparing"))return;Thread.sleep(100);}
        fail("Host did not reach expected on-air state: "+expected);
    }
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
                // Manual details stay behind the current Advanced disclosure, including saved addresses.
                text(device,"Advanced connection settings").click();
                text(device,"Connect").click();
                assertTrue(device.wait(Until.hasObject(By.textStartsWith("Host connected.")),12000));
                text(device,"Advanced connection settings").click();
                description(device,"AMP Cast admin password").setText("test-secret");
                text(device,"Connect").click();
                assertTrue(device.wait(Until.hasObject(By.textStartsWith("Host connected.")),12000));
                scenario.onActivity(a->((android.view.inputmethod.InputMethodManager)a.getSystemService(Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(a.getWindow().getDecorView().getWindowToken(),0));
                text(device,"Start host station").click();
                awaitOnAir(client,true);
                text(device,"Listen to host on this phone");
                assertTrue(device.wait(Until.hasObject(By.text("Listen to host on this phone").enabled(true)),12000));
                device.findObject(By.text("Listen to host on this phone")).click();
                long until=System.currentTimeMillis()+12000;boolean[] active={false};
                while(System.currentTimeMillis()<until){main(()->active[0]=player[0].isPlaying());if(active[0])break;Thread.sleep(100);}
                assertTrue("Real server MP3 must play through phone service",active[0]);
                text(device,"Stop phone playback").click();assertTrue(client.status().getBoolean("on_air"));
                text(device,"Stop host program for all listeners").click();awaitOnAir(client,false);
            }
        }finally {
            client.command(false);
            if(player[0]!=null)main(()->{player[0].stop();if(saved.isEmpty())player[0].clearMediaItems();else player[0].setMediaItems(saved,index[0],position[0]);player[0].setVolume(volume[0]);if(playing[0]&&!saved.isEmpty()){player[0].prepare();player[0].play();}player[0].release();});
            if(savedServer==null)prefs.edit().remove("cast-server").commit();else prefs.edit().putString("cast-server",savedServer).commit();
        }
    }
    @Test public void rejectsAmbiguousServerAddresses()throws Exception {
        for(String value:new String[]{"file:///music","http://user:secret@localhost:8787","http://localhost:8787/api/status","http://localhost:8787?station=other"}){
            try{new CastClient(value,"");fail("Rejected address expected");}catch(Exception expected){assertTrue(expected.getMessage().contains("server address"));}
        }
    }
}
