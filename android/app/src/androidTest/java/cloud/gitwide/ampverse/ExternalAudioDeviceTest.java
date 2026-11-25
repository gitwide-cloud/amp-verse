package cloud.gitwide.ampverse;
import android.content.*;
import android.content.pm.PackageManager;
import android.net.Uri;
import androidx.media3.common.Player;
import androidx.test.core.app.ActivityScenario;
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
        instrumentation.getContext().grantUriPermission(context.getPackageName(),uri,Intent.FLAG_GRANT_READ_URI_PERMISSION);
        int saved=LibraryStore.read(context).size();
        try(ActivityScenario<MainActivity> activity=ActivityScenario.launch(audio(uri).setPackage(context.getPackageName()))){
            awaitPlaying();
            assertEquals(saved,LibraryStore.read(context).size());
            activity.moveToState(androidx.lifecycle.Lifecycle.State.CREATED);
            Thread.sleep(300);
            awaitPlaying();
            activity.moveToState(androidx.lifecycle.Lifecycle.State.RESUMED);
            // A second request reaches the existing activity; bad access must leave playback intact.
            activity.onActivity(a->a.onNewIntent(audio(Uri.parse("content://cloud.gitwide.ampverse.test.external/missing"))));
            Thread.sleep(300);awaitPlaying();
            assertEquals(saved,LibraryStore.read(context).size());
        }finally{instrumentation.runOnMainSync(()->{Player p=PlaybackService.player();if(p!=null){p.pause();p.clearMediaItems();}});instrumentation.getContext().revokeUriPermission(uri,Intent.FLAG_GRANT_READ_URI_PERMISSION);}
    }
    private void awaitPlaying()throws Exception{
        long deadline=System.currentTimeMillis()+12000;boolean[] ready={false};
        do{instrumentation.runOnMainSync(()->{Player p=PlaybackService.player();ready[0]=p!=null&&p.isPlaying()&&p.getCurrentMediaItem()!=null&&"external-audio".equals(p.getCurrentMediaItem().mediaId);});if(ready[0])return;Thread.sleep(100);}while(System.currentTimeMillis()<deadline);
        fail("Open-with did not produce active external-audio playback");
    }
}
