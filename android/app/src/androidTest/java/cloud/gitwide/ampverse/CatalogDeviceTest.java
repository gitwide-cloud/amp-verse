package cloud.gitwide.ampverse;
import android.content.*;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.media3.common.*;
import org.json.*;
import org.junit.Test;
import java.io.*;
import java.util.*;
import static org.junit.Assert.*;
public class CatalogDeviceTest {
    @Test public void oldJsonIsRetiredOnlyAfterVerifiedLocalCopy()throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        File folder=new File(context.getCacheDir(),"migration-"+UUID.randomUUID());File track=new File(folder,"ampdj/tracks/song.wav.json");track.getParentFile().mkdirs();
        java.nio.file.Files.write(track.toPath(),"{\"cue_in\":3.5,\"manual_override\":true}".getBytes(java.nio.charset.StandardCharsets.UTF_8));
        try{CatalogStore.migrateFolder(folder,false);assertFalse(track.exists());assertEquals(3.5,new JSONObject(CatalogStore.get(new File(folder,"ampdj/catalog.ampdb"),"tracks/song.wav.json")).getDouble("cue_in"),0);}
        finally{try(java.util.stream.Stream<java.nio.file.Path> paths=java.nio.file.Files.walk(folder.toPath())){paths.sorted(java.util.Comparator.reverseOrder()).forEach(p->p.toFile().delete());}}
    }
    @Test public void embeddedCatalogAndPreparedCuesWorkWithoutServer()throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        File file=File.createTempFile("amp-catalog-test-",".ampdb",context.getCacheDir());file.delete();
        try {
            CatalogStore.put(file,"tracks/song.wav.json","{\"cue_in\":2.5,\"cue_out\":12,\"bpm\":128,\"manual_cues\":[{\"number\":1,\"seconds\":3}]}");
            JSONObject prep=new JSONObject(CatalogStore.get(file,"tracks/song.wav.json"));
            assertEquals(128,prep.getInt("bpm"));
            MediaItem item=new MediaItem.Builder().setMediaId("test").setMediaMetadata(new MediaMetadata.Builder().setExtras(DjEngine.preparedExtras(prep.toString())).build()).build();
            assertEquals(2500,DjEngine.cueIn(item));assertEquals(12000,DjEngine.cueOut(item,20000));
            CatalogStore.put(file,"tracks/song.wav.json","{\"cue_in\":4}");
            assertEquals(4,new JSONObject(CatalogStore.get(file,"tracks/song.wav.json")).getInt("cue_in"));
        } finally {file.delete();}
    }
    @Test public void freshInstallContainsOnlyDemo()throws Exception {
        Context target=InstrumentationRegistry.getInstrumentation().getTargetContext();String token="demo-test-"+UUID.randomUUID();
        Context isolated=new ContextWrapper(target){@Override public SharedPreferences getSharedPreferences(String name,int mode){return target.getSharedPreferences(token+name,mode);}};
        try {JSONArray lists=PhonePlaylists.read(isolated);assertEquals(1,lists.length());assertEquals("Demo",lists.getJSONObject(0).getString("name"));assertEquals(0,lists.getJSONObject(0).getJSONArray("songs").length());}
        finally{target.deleteSharedPreferences(token+"phone-playlists");}
    }
}
