package cloud.gitwide.ampverse;
import android.net.Uri;
import android.provider.DocumentsContract;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.util.List;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public class FolderScanDeviceTest {
    @Test public void scansNestedAudioAndSkipsOtherFilesAndCycles()throws Exception{
        Uri root=DocumentsContract.buildTreeDocumentUri("cloud.gitwide.ampverse.test.documents","root");
        InstrumentationRegistry.getInstrumentation().getUiAutomation().adoptShellPermissionIdentity("android.permission.MANAGE_DOCUMENTS");
        try {
        List<Uri> audio=FolderScanner.scan(InstrumentationRegistry.getInstrumentation().getTargetContext().getContentResolver(),root);
        assertEquals(2,audio.size());
        assertEquals("song-a",DocumentsContract.getDocumentId(audio.get(0)));
        assertEquals("song-b",DocumentsContract.getDocumentId(audio.get(1)));
        assertFalse(FolderScanner.audio("notes.txt","text/plain"));
        for(Uri song:audio){org.json.JSONObject prep=new org.json.JSONObject(FolderScanner.preparation(InstrumentationRegistry.getInstrumentation().getTargetContext().getContentResolver(),root,song));assertEquals(7.25,prep.getDouble("cue_in"),0.001);assertEquals(128,prep.getInt("bpm"));assertTrue(prep.getBoolean("manual_override"));}
        }finally{InstrumentationRegistry.getInstrumentation().getUiAutomation().dropShellPermissionIdentity();}
    }
}
