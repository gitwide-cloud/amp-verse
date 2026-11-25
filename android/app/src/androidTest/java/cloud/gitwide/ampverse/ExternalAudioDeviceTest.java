package cloud.gitwide.ampverse;
import android.content.*;
import android.content.pm.PackageManager;
import android.net.Uri;
import androidx.media3.common.Player;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public class ExternalAudioDeviceTest {
    private final android.app.Instrumentation instrumentation=InstrumentationRegistry.getInstrumentation();
    private final Context context=instrumentation.getTargetContext();
    private Intent audio(Uri uri){return new Intent(Intent.ACTION_VIEW).setDataAndType(uri,"audio/wav").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_GRANT_READ_URI_PERMISSION);}
    @Test public void resolverAdvertisesMusicAndAudioButNotArbitraryLinks(){
        PackageManager pm=context.getPackageManager();
        assertNotNull(pm.resolveActivity(audio(Uri.parse("content://media/external/audio/media/1")).setPackage(context.getPackageName()),PackageManager.MATCH_DEFAULT_ONLY));
        assertNotNull(pm.resolveActivity(new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_APP_MUSIC).setPackage(context.getPackageName()),PackageManager.MATCH_DEFAULT_ONLY));
        assertNull(pm.resolveActivity(new Intent(Intent.ACTION_VIEW).setDataAndType(Uri.parse("https://example.org/song.mp3"),"audio/mpeg").setPackage(context.getPackageName()),PackageManager.MATCH_DEFAULT_ONLY));
        assertFalse(ExternalAudio.accepts(new Intent(Intent.ACTION_VIEW).setDataAndType(Uri.parse("content://media/a"),"text/plain")));
    }
    @Test public void openWithPlaysAcrossBackgroundAndKeepsLibrary()throws Exception{
        Uri uri=Uri.parse("content://cloud.gitwide.ampverse.test.external/song");
        int saved=LibraryStore.read(context).size();
        Intent sender=new Intent().setComponent(new ComponentName(instrumentation.getContext().getPackageName(),ExternalAudioSenderActivity.class.getName())).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try{
            context.startActivity(sender);awaitPlaying();
            assertEquals(saved,LibraryStore.read(context).size());
            // Close the actual receiver Activity: only the playback-service grant should remain.
            instrumentation.runOnMainSync(()->{
                for(android.app.Activity a:androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry.getInstance().getActivitiesInStage(androidx.test.runner.lifecycle.Stage.RESUMED))if(a instanceof MainActivity)a.finish();
            });
            Thread.sleep(300);
            assertEquals(PackageManager.PERMISSION_GRANTED,context.checkUriPermission(uri,android.os.Process.myPid(),android.os.Process.myUid(),Intent.FLAG_GRANT_READ_URI_PERMISSION));
            awaitPlaying();
            // An unreadable second open must not replace the playing file or change the library.
            context.startActivity(new Intent(sender).putExtra("missing",true));
            Thread.sleep(500);awaitPlaying();
            assertEquals(saved,LibraryStore.read(context).size());
        }finally{instrumentation.runOnMainSync(()->{Player p=PlaybackService.player();if(p!=null){p.pause();p.clearMediaItems();}});}
    }
    private void awaitPlaying()throws Exception{
        long deadline=System.currentTimeMillis()+12000;boolean[] ready={false};
        do{instrumentation.runOnMainSync(()->{Player p=PlaybackService.player();ready[0]=p!=null&&p.isPlaying()&&p.getCurrentMediaItem()!=null&&"external-audio".equals(p.getCurrentMediaItem().mediaId);});if(ready[0])return;Thread.sleep(100);}while(System.currentTimeMillis()<deadline);
        fail("Open-with did not produce active external-audio playback");
    }
}
