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
import java.io.File;
import java.nio.*;
import java.nio.file.Files;
import java.util.*;
import java.util.concurrent.TimeUnit;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
public class MusicSearchDeviceTest {
    private void main(Runnable action){InstrumentationRegistry.getInstrumentation().runOnMainSync(action);}
    @Test public void filteredSongStillPlaysItsOriginalQueueIndex()throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        List<MediaItem> library=LibraryStore.read(context),session=new ArrayList<>();
        MediaController[] controller=new MediaController[1];float[] volume={1};long[] position={0};int[] index={0};boolean[] playing={false};
        File folder=new File(context.getFilesDir(),"audio");folder.mkdirs();String prefix="search-test-"+UUID.randomUUID();File alpha=new File(folder,prefix+"-alpha.wav"),beta=new File(folder,prefix+"-beta.wav");
        int bytes=16000*2;ByteBuffer wav=ByteBuffer.allocate(44+bytes).order(ByteOrder.LITTLE_ENDIAN);
        wav.put("RIFF".getBytes()).putInt(36+bytes).put("WAVEfmt ".getBytes()).putInt(16).putShort((short)1).putShort((short)1).putInt(8000).putInt(16000).putShort((short)2).putShort((short)16).put("data".getBytes()).putInt(bytes);
        Files.write(alpha.toPath(),wav.array());Files.write(beta.toPath(),wav.array());
        try {
            com.google.common.util.concurrent.ListenableFuture<MediaController>[] future=new com.google.common.util.concurrent.ListenableFuture[1];
            main(()->future[0]=new MediaController.Builder(context,new SessionToken(context,new ComponentName(context,PlaybackService.class))).buildAsync());controller[0]=future[0].get(10,TimeUnit.SECONDS);
            main(()->{for(int i=0;i<controller[0].getMediaItemCount();i++)session.add(controller[0].getMediaItemAt(i));volume[0]=controller[0].getVolume();position[0]=controller[0].getCurrentPosition();index[0]=Math.max(0,controller[0].getCurrentMediaItemIndex());playing[0]=controller[0].getPlayWhenReady();controller[0].pause();controller[0].setVolume(0);});
            LibraryStore.write(context,Arrays.asList(LibraryStore.item(alpha,"Alpha dance"),LibraryStore.item(beta,"Beta chill")));
            Intent intent=new Intent(context,MainActivity.class).putExtra("module","AMP Library").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(intent)) {
                UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
                assertTrue(device.wait(Until.hasObject(By.desc("Search phone music")),5000));
                device.findObject(By.desc("Search phone music")).setText("beta");
                assertTrue(device.wait(Until.hasObject(By.text("2. Beta chill")),3000));assertFalse(device.hasObject(By.text("1. Alpha dance")));
                device.findObject(By.text("2. Beta chill")).click();
                String[] selected={""}; long deadline=android.os.SystemClock.uptimeMillis()+5000; do { main(()->selected[0]=controller[0].getCurrentMediaItem()==null?"":controller[0].getCurrentMediaItem().mediaId); if(beta.getName().equals(selected[0]))break; android.os.SystemClock.sleep(50); } while(android.os.SystemClock.uptimeMillis()<deadline); assertEquals(beta.getName(),selected[0]);
            }
        }finally {
            LibraryStore.write(context,library);
            if(controller[0]!=null)main(()->{controller[0].stop();if(session.isEmpty())controller[0].clearMediaItems();else controller[0].setMediaItems(session,index[0],position[0]);controller[0].setVolume(volume[0]);if(playing[0]&&!session.isEmpty()){controller[0].prepare();controller[0].play();}controller[0].release();});
            alpha.delete();beta.delete();
        }
    }
}
