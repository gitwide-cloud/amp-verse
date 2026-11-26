package cloud.gitwide.ampverse;

import android.content.*;
import android.view.*;
import android.widget.*;
import androidx.media3.common.MediaItem;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import java.util.*;
import org.junit.*;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
public class DjLoadPadsDeviceTest {
    private int tags(View view,String prefix){int count=view.getTag() instanceof String&&((String)view.getTag()).startsWith(prefix)?1:0;if(view instanceof ViewGroup)for(int i=0;i<((ViewGroup)view).getChildCount();i++)count+=tags(((ViewGroup)view).getChildAt(i),prefix);return count;}
    @Test public void distinctHotCueAndSamplerBanksAndQueuedSongsRemainLoadable()throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        Intent intent=new Intent(context,MainActivity.class).putExtra("module","AMP DJ").putExtra("phone-ui",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(intent)){
            long end=android.os.SystemClock.elapsedRealtime()+6000;while(PlaybackService.dj()==null&&android.os.SystemClock.elapsedRealtime()<end)Thread.sleep(40);
            scenario.onActivity(activity->{try{
                DjEngine engine=PlaybackService.dj();assertNotNull(engine);
                DjPanel panel=new DjPanel(activity,new Appearance(activity),engine,new DjPanel.Host(){public void effect(int side,int pad){}public void beforeMainPlay(){}public void equalizer(){}});
                assertEquals("One hot-cue bank",1,tags(panel,"dj-hot-cue-bank"));assertEquals("Eight actual hot cues",8,tags(panel,"dj-hot-cue-" )-1);
                assertEquals("One sampler bank",1,tags(panel,"dj-sampler-bank"));assertEquals("Nine sampler pads",9,tags(panel,"dj-sample-"));
                List<MediaItem> prior=new ArrayList<>(engine.autoQueue);MediaItem queued=new MediaItem.Builder().setMediaId("load-regression-queued").setUri("file:///not-played.mp3").setMediaMetadata(new androidx.media3.common.MediaMetadata.Builder().setTitle("Queued song").build()).build();
                try{engine.autoQueue.add(queued);java.lang.reflect.Method method=DjPanel.class.getDeclaredMethod("stationAndPhoneSongs");method.setAccessible(true);List<?> songs=(List<?>)method.invoke(panel);assertTrue("Load must include the current DJ playlist, even when not downloaded",songs.contains(queued));}finally{engine.autoQueue.clear();engine.autoQueue.addAll(prior);}
            }catch(Exception error){throw new AssertionError(error);}});
        }
    }
}
