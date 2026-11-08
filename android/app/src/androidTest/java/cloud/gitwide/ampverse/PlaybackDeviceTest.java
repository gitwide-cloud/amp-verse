package cloud.gitwide.ampverse;

import android.content.ComponentName;
import android.content.Context;
import androidx.media3.common.MediaItem;
import androidx.media3.common.Player;
import androidx.media3.session.MediaController;
import androidx.media3.session.SessionToken;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.io.*;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.*;
import java.util.concurrent.TimeUnit;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public class PlaybackDeviceTest {
    private void main(Runnable action) { InstrumentationRegistry.getInstrumentation().runOnMainSync(action); }
    private File wave(Context context, String name) throws Exception {
        File directory = new File(context.getFilesDir(), "audio"); directory.mkdirs();
        File file = new File(directory, name);
        int dataSize = 44100 * 2 * 12;
        ByteBuffer wave = ByteBuffer.allocate(44 + dataSize).order(ByteOrder.LITTLE_ENDIAN);
        wave.put("RIFF".getBytes()).putInt(36 + dataSize).put("WAVEfmt ".getBytes()).putInt(16).putShort((short)1).putShort((short)1).putInt(44100).putInt(88200).putShort((short)2).putShort((short)16).put("data".getBytes()).putInt(dataSize);
        try (FileOutputStream stream = new FileOutputStream(file)) { stream.write(wave.array()); }
        return file;
    }
    @Test public void localQueuePersistsAndPlaybackContinuesWithScreenOff() throws Exception {
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        List<MediaItem> saved = LibraryStore.read(context);
        File first = wave(context, "amp-qa-first.wav"), second = wave(context, "amp-qa-second.wav");
        List<MediaItem> fixtures = Arrays.asList(LibraryStore.item(first, "QA first"), LibraryStore.item(second, "QA second"));
        MediaController[] controller = new MediaController[1];
        ActivityScenario<MainActivity> scenario = null;
        try {
            LibraryStore.write(context, fixtures);
            assertEquals(2, LibraryStore.read(context).size());
            assertEquals("QA second", LibraryStore.read(context).get(1).mediaMetadata.title.toString());
            scenario = ActivityScenario.launch(MainActivity.class);
            com.google.common.util.concurrent.ListenableFuture<MediaController>[] future = new com.google.common.util.concurrent.ListenableFuture[1];
            main(() -> future[0] = new MediaController.Builder(context, new SessionToken(context, new ComponentName(context, PlaybackService.class))).buildAsync());
            controller[0] = future[0].get(10, TimeUnit.SECONDS);
            main(() -> { controller[0].setMediaItems(fixtures); controller[0].setRepeatMode(Player.REPEAT_MODE_OFF); controller[0].setVolume(0); controller[0].prepare(); controller[0].play(); });
            Thread.sleep(1800);
            main(() -> { assertTrue(controller[0].isPlaying()); assertTrue(controller[0].getCurrentPosition() > 500); controller[0].seekTo(0, 11500); });
            Thread.sleep(1400);
            main(() -> { assertEquals(1, controller[0].getCurrentMediaItemIndex()); assertTrue(controller[0].isPlaying()); });
            // Device-level test, no radio/network dependencies. Wake again in finally.
            InstrumentationRegistry.getInstrumentation().getUiAutomation().executeShellCommand("input keyevent 223").close();
            long[] before = new long[1]; main(() -> before[0] = controller[0].getCurrentPosition());
            Thread.sleep(2500);
            main(() -> { assertTrue(controller[0].isPlaying()); assertTrue(controller[0].getCurrentPosition() > before[0] + 1500); });
            main(() -> { controller[0].pause(); assertFalse(controller[0].getPlayWhenReady()); });
        } finally {
            InstrumentationRegistry.getInstrumentation().getUiAutomation().executeShellCommand("input keyevent 224").close();
            if (controller[0] != null) main(() -> { controller[0].stop(); controller[0].setMediaItems(saved); controller[0].setVolume(1); controller[0].release(); });
            if (scenario != null) scenario.close();
            LibraryStore.write(context, saved); first.delete(); second.delete();
        }
    }
}
