package cloud.gitwide.ampverse;

import android.content.*;
import androidx.media3.common.MediaItem;
import androidx.media3.exoplayer.ExoPlayer;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.uiautomator.*;
import org.junit.*;
import org.junit.runner.RunWith;
import org.json.*;
import java.io.*;
import java.nio.*;
import java.nio.file.Files;
import java.util.*;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
public class DjDeviceTest {
    private void main(Runnable action){InstrumentationRegistry.getInstrumentation().runOnMainSync(action);}
    private Context context(){return InstrumentationRegistry.getInstrumentation().getTargetContext();}
    private File fixture(String name)throws Exception{File folder=new File(context().getFilesDir(),"audio");folder.mkdirs();File file=new File(folder,name+".wav");int bytes=8000*2*14;ByteBuffer wav=ByteBuffer.allocate(44+bytes).order(ByteOrder.LITTLE_ENDIAN);wav.put("RIFF".getBytes()).putInt(36+bytes).put("WAVEfmt ".getBytes()).putInt(16).putShort((short)1).putShort((short)1).putInt(8000).putInt(16000).putShort((short)2).putShort((short)16).put("data".getBytes()).putInt(bytes);Files.write(file.toPath(),wav.array());return file;}
    private void waitFor(java.util.function.BooleanSupplier predicate)throws Exception{long end=android.os.SystemClock.uptimeMillis()+6000;while(!predicate.getAsBoolean()){if(android.os.SystemClock.uptimeMillis()>end)fail("Timed out waiting for audio state");Thread.sleep(30);}}
    @Test public void nativeDecksCueStagingAndActivityRecreation()throws Exception{
        File a=fixture("dj-test-"+UUID.randomUUID()),b=fixture("dj-test-"+UUID.randomUUID());
        MediaItem alpha=LibraryStore.item(a,"DJ test Alpha"),beta=LibraryStore.item(b,"DJ test Beta");
        DjEngine[] dj=new DjEngine[1];MediaItem[] prior=new MediaItem[4];long[] positions=new long[4];boolean[] playing=new boolean[4];float[] volumes=new float[4];boolean[] modes=new boolean[2],collapsed=new boolean[2];float[] fader={0},master={.75f};
        UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
        Intent intent=new Intent(context(),MainActivity.class).putExtra("module","AMP DJ").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(intent)){
            scenario.onActivity(activity->{activity.getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);activity.setShowWhenLocked(true);activity.setTurnScreenOn(true);});
            waitFor(()->{main(()->dj[0]=PlaybackService.dj());return dj[0]!=null;});
            main(()->{DjEngine e=dj[0];fader[0]=e.fader;master[0]=e.master;e.master=0;for(int i=0;i<4;i++){prior[i]=e.decks[i].getCurrentMediaItem();positions[i]=e.decks[i].getCurrentPosition();playing[i]=e.decks[i].getPlayWhenReady();volumes[i]=e.decks[i].getVolume();e.decks[i].pause();e.decks[i].setVolume(0);}for(int s=0;s<2;s++){modes[s]=e.cueMode[s];collapsed[s]=e.collapsed[s];}e.decks[0].setPlaybackParameters(new androidx.media3.common.PlaybackParameters(1.07f,1));e.load(0,alpha,0);assertEquals(1f,e.decks[0].getPlaybackParameters().speed,0);e.load(1,beta,0);e.load(2,beta,0);e.decks[0].play();e.decks[2].play();});
            boolean[] running={false};waitFor(()->{main(()->running[0]=dj[0].decks[0].isPlaying()&&dj[0].decks[2].isPlaying());return running[0];});
            main(()->{DjEngine e=dj[0];e.load(0,beta,4000);assertSame(beta,e.staged[0]);assertEquals(alpha.mediaId,e.decks[0].getCurrentMediaItem().mediaId);e.mode(0,true);e.collapse(0);e.seek(2,2000);assertTrue(e.active(0));assertTrue(e.active(2));});
            scenario.recreate();
            main(()->{assertSame(dj[0],PlaybackService.dj());assertTrue(dj[0].active(0));assertTrue(dj[0].active(2));dj[0].stop(0);dj[0].apply(0);assertEquals(beta.mediaId,dj[0].decks[0].getCurrentMediaItem().mediaId);assertFalse(dj[0].active(0));assertTrue(dj[0].active(2));});
            assertTrue(device.wait(Until.hasObject(By.text("DECK A")),3000));
            main(()->{DjEngine e=dj[0];e.stop(0);e.stop(1);e.load(0,alpha,6000);e.setFader(0);e.play(0);e.queue(Arrays.asList(alpha,beta));e.auto(true);assertTrue(e.auto);});
            long end=android.os.SystemClock.uptimeMillis()+11000;boolean[] mixed={false};do{main(()->mixed[0]=!dj[0].mixing()&&dj[0].fader==1f);if(mixed[0])break;Thread.sleep(50);}while(android.os.SystemClock.uptimeMillis()<end);assertTrue("Crossfade must finish",mixed[0]);
            main(()->{assertFalse(dj[0].active(0));assertTrue(dj[0].active(1));});
            main(()->{DjEngine e=dj[0];e.autoQueue.clear();e.autoQueue.add(alpha);e.autoQueue.add(beta);e.autoQueue.add(alpha);e.decks[0].clearMediaItems();e.mixNext();assertTrue("Auto crossfade prepares the queued song",e.mixing());assertEquals(alpha.mediaId,e.decks[0].getCurrentMediaItem().mediaId);e.auto(false);assertFalse("Auto Mix off freezes automatic fades",e.mixing());});
            main(()->{DjEngine e=dj[0];e.auto(false);e.cancelMix();e.autoQueue.clear();for(int i=0;i<4;i++){e.decks[i].pause();e.decks[i].clearMediaItems();if(prior[i]!=null){e.decks[i].setMediaItem(prior[i],positions[i]);e.decks[i].prepare();e.decks[i].setPlayWhenReady(playing[i]);}e.decks[i].setVolume(volumes[i]);e.staged[i]=null;}e.fader=fader[0];e.master=master[0];for(int s=0;s<2;s++){e.mode(s,modes[s]);if(e.collapsed[s]!=collapsed[s])e.collapse(s);}e.checkpoint();});
        }finally{if(dj[0]!=null)main(()->{for(ExoPlayer deck:dj[0].decks)if(deck.getCurrentMediaItem()!=null&&(deck.getCurrentMediaItem().mediaId.equals(alpha.mediaId)||deck.getCurrentMediaItem().mediaId.equals(beta.mediaId))){deck.stop();deck.clearMediaItems();}});a.delete();b.delete();}
    }
    @Test public void sourcePlaylistsIncludeAllSongsAndKeepLocalOverrides()throws Exception{
        File a=fixture("playlist-test-"+UUID.randomUUID()),b=fixture("playlist-test-"+UUID.randomUUID());List<MediaItem> saved=LibraryStore.read(context());boolean hadLists=context().getSharedPreferences("phone-playlists",0).contains("lists");String previous=context().getSharedPreferences("phone-playlists",0).getString("lists","[]");String prefix="test-"+UUID.randomUUID();
        try{MediaItem alpha=LibraryStore.item(a,"Alpha"),beta=LibraryStore.item(b,"Beta");LibraryStore.write(context(),Arrays.asList(alpha,beta));JSONObject list=new JSONObject().put("id",prefix).put("name","Source").put("songs",new JSONArray().put(beta.mediaId).put(alpha.mediaId));PhonePlaylists.save(context(),list);JSONObject smart=new JSONObject().put("id",prefix+"-smart").put("name","Mix").put("smart",true).put("source",prefix).put("style","Steady");assertEquals(2,PhonePlaylists.tracks(context(),smart).size());smart.put("source","");assertTrue(PhonePlaylists.tracks(context(),smart).isEmpty());assertEquals(beta.mediaId,PhonePlaylists.tracks(context(),list).get(0).mediaId);PhonePlaylists.remove(context(),prefix);assertFalse(PhonePlaylists.read(context()).toString().contains(prefix));assertEquals("[]",context().getSharedPreferences("phone-cues",0).getString(prefix,"[]"));}
        finally{LibraryStore.write(context(),saved);if(hadLists)context().getSharedPreferences("phone-playlists",0).edit().putString("lists",previous).commit();else context().getSharedPreferences("phone-playlists",0).edit().remove("lists").commit();a.delete();b.delete();}
    }
}
