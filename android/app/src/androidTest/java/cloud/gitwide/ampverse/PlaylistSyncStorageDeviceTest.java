package cloud.gitwide.ampverse;

import android.content.Context;
import android.util.AtomicFile;
import androidx.test.core.app.ApplicationProvider;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.UUID;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;

/** Isolated private test key only; never selects a binding or starts network/playback. */
@RunWith(AndroidJUnit4.class)
public class PlaylistSyncStorageDeviceTest {
    @Test public void backupOnlyStateRestoresPendingOperationWithoutReplacingIt()throws Exception {
        Context context=ApplicationProvider.getApplicationContext();
        String key=PlaylistSync.hash("storage-fixture-"+UUID.randomUUID());
        File folder=new File(context.getNoBackupFilesDir(),"playlist-sync");
        assertTrue(folder.isDirectory()||folder.mkdirs());
        File base=new File(folder,key+".json"),backup=new File(base.getPath()+".bak");
        try{
            JSONObject state=PlaylistSyncModel.state("fixture-server","fixture-library","fixture-device");
            state.getJSONObject("documents").put("normal_fixture",new JSONObject().put("pending",new JSONObject().put("operation_id","exact-pending-operation")));
            AtomicFile file=new AtomicFile(base);
            FileOutputStream output=file.startWrite();
            try{output.write(state.toString().getBytes(StandardCharsets.UTF_8));output.getFD().sync();file.finishWrite(output);}
            catch(Exception e){file.failWrite(output);throw e;}
            assertTrue(base.renameTo(backup));assertFalse(base.exists());
            JSONObject restored=PlaylistSync.read(context,key);
            assertEquals("exact-pending-operation",restored.getJSONObject("documents").getJSONObject("normal_fixture").getJSONObject("pending").getString("operation_id"));
            assertFalse(restored.has("playback"));assertFalse(restored.has("preparation"));
        }finally{
            for(String suffix:new String[]{"",".bak",".new"})new File(base.getPath()+suffix).delete();
        }
    }
}
