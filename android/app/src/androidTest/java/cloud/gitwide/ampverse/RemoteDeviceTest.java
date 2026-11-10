package cloud.gitwide.ampverse;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.*;
import org.junit.runner.RunWith;
import org.json.*;
import java.util.UUID;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
public class RemoteDeviceTest {
    @Test public void isolatedDesktopAcceptsAndroidCommandsAndCueDeck()throws Exception {
        String base=InstrumentationRegistry.getArguments().getString("desktop_url");Assume.assumeTrue("Supply isolated desktop_url via adb reverse",base!=null);
        JSONObject catalog=RemotePanel.request(base,"","/api/v1/catalog",null);JSONArray songs=catalog.getJSONArray("songs");assertTrue(songs.length()>0);String song=songs.getJSONObject(0).getString("id");
        JSONObject request=new JSONObject().put("id","android-test-"+UUID.randomUUID()).put("action","call_song").put("song",song).put("deck","a").put("execution","load_only");assertTrue(RemotePanel.request(base,"","/api/v1/command",request).getBoolean("ok"));
        // A retry keeps its original request ID; the server must acknowledge it without re-execution.
        assertTrue(RemotePanel.request(base,"","/api/v1/command",request).getBoolean("ok"));
        JSONObject cue=new JSONObject().put("id","android-test-"+UUID.randomUUID()).put("action","cue_deck").put("side",1).put("operation",new JSONObject().put("operation","load").put("song",song));assertTrue(RemotePanel.request(base,"","/api/v1/command",cue).getBoolean("ok"));
        JSONObject state=RemotePanel.request(base,"","/api/v1/state",null);assertFalse(state.getJSONArray("decks").getJSONObject(0).getBoolean("playing"));assertTrue(state.getJSONArray("cue_decks").getJSONObject(1).getBoolean("loaded"));
        JSONObject invalid=new JSONObject().put("id","android-test-"+UUID.randomUUID()).put("action","side_sampler").put("side",2).put("pad",10);try{RemotePanel.request(base,"","/api/v1/command",invalid);fail("Invalid sampler destination accepted");}catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("409"));}
    }
}
