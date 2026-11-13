package cloud.gitwide.ampverse;
import android.content.*;
import android.net.Uri;
import androidx.media3.common.MediaItem;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
public class ManagedLibraryDeviceTest {
 @Test public void localReferencesRoundTripWithoutCopyingOrMoving()throws Exception{
  Context app=InstrumentationRegistry.getInstrumentation().getTargetContext();File sandbox=new File(app.getCacheDir(),"local-refs-"+UUID.randomUUID());assertTrue(sandbox.mkdir());
  Context isolated=new ContextWrapper(app){@Override public File getFilesDir(){return sandbox;}};
  try{
   Uri original=Uri.parse("content://example.documents/tree/music/document/original");
   MediaItem item=LibraryStore.reference(isolated,original,"Original song",new JSONObject().put("bpm",128).put("cue_in",3.5).toString());
   LibraryStore.write(isolated,Arrays.asList(item));List<MediaItem> read=LibraryStore.read(isolated);
   assertEquals(1,read.size());assertEquals(original,read.get(0).localConfiguration.uri);assertEquals(item.mediaId,read.get(0).mediaId);assertEquals(3500,DjLogic.cueIn(read.get(0)));assertFalse(new File(sandbox,"audio").exists());
   LibraryStore.write(isolated,Collections.emptyList());assertTrue(LibraryStore.read(isolated).isEmpty());
  }finally{try(java.util.stream.Stream<Path> paths=Files.walk(sandbox.toPath())){paths.sorted(Comparator.reverseOrder()).forEach(path->path.toFile().delete());}}
 }
 private MediaItem song(double bpm)throws Exception{return new MediaItem.Builder().setUri("content://example/song").setMediaMetadata(new androidx.media3.common.MediaMetadata.Builder().setExtras(DjEngine.preparedExtras(new JSONObject().put("bpm",bpm).toString())).build()).build();}
 @Test public void lightweightLogicUsesPreparedTempoAndConservativeLimits()throws Exception{
  assertEquals(1.04f,DjLogic.tempoRatio(song(130),song(125),1),.001f);
  assertEquals(1f,DjLogic.tempoRatio(song(130),song(0),1),0);
  assertEquals(1f,DjLogic.tempoRatio(song(130),song(100),1),0);
  assertEquals(1f,DjLogic.tempoRatio(song(128),song(64),1),0);
 }
}
