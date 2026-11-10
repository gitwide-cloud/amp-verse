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
                assertTrue(device.wait(Until.hasObject(By.text("Server settings / enter address")),5000));
                device.findObject(By.text("Server settings / enter address")).click();
                assertTrue(device.wait(Until.hasObject(By.text("Connect")),5000));
                device.findObject(By.text("Connect")).click();
                assertTrue(device.wait(Until.hasObject(By.text("Station connected")),12000));
                device.findObject(By.text("Server settings / enter address")).click();
                device.findObject(By.desc("AMP Cast admin password")).setText("test-secret");
                device.findObject(By.text("Connect")).click();
                assertTrue(device.wait(Until.hasObject(By.text("Station connected")),12000));
                device.findObject(By.desc("Search station songs")).setText("zzzz-no-match");
                assertTrue(device.wait(Until.hasObject(By.text("No songs match your search.")),3000));
                device.findObject(By.desc("Search station songs")).setText("phone-test");
                assertTrue(device.wait(Until.hasObject(By.textContains("Phone-test")),3000));
                device.findObject(By.desc("Search station songs")).setText("");
                scenario.onActivity(a->((android.view.inputmethod.InputMethodManager)a.getSystemService(Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(a.getWindow().getDecorView().getWindowToken(),0));
                device.findObject(By.text("Server settings / enter address")).click();
                device.findObject(By.text("Start station")).click();
                assertTrue(device.wait(Until.hasObject(By.text("Station started")),12000));assertTrue(client.status().getBoolean("running"));
                device.findObject(By.text("Listen on this phone")).click();
                long until=System.currentTimeMillis()+12000;boolean[] active={false};
                while(System.currentTimeMillis()<until){main(()->active[0]=player[0].isPlaying());if(active[0])break;Thread.sleep(100);}
                assertTrue("Real server MP3 must play through phone service",active[0]);
                device.findObject(By.text("Stop phone playback")).click();assertTrue(client.status().getBoolean("running"));
                device.findObject(By.text("Stop station")).click();assertTrue(device.wait(Until.hasObject(By.text("Station stopped")),12000));assertFalse(client.status().getBoolean("running"));
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
