package cloud.gitwide.ampverse;

import org.json.*;
import org.junit.Test;
import static org.junit.Assert.*;

public class OfflineSongMetadataTest {
    @Test public void checksumMigratesLegacyButPrefersBasicIntegrity() throws Exception {
        JSONObject old = new JSONObject().put("preparation", new JSONObject().put("source_sha256", "legacy"));
        assertEquals("legacy", OfflineSongMetadata.checksum(old));
        old.put("sha256", "current");
        assertEquals("current", OfflineSongMetadata.checksum(old));
        assertEquals("", OfflineSongMetadata.checksum(new JSONObject()));
    }
    @Test public void basicRefreshPreservesManualPreparationAndRemoteBaseline() throws Exception {
        JSONObject old = new JSONObject().put("preparation", new JSONObject().put("bpm",129).put("manual_override",true))
            .put("remote_preparation",new JSONObject().put("bpm",128)).put("revision",new JSONObject().put("old",1));
        JSONObject item = new JSONObject(old.toString());
        OfflineSongMetadata.mergePreparation(item, old, null, "new-sha", "song-key", new JSONArray());
        assertEquals(129,item.getJSONObject("preparation").getInt("bpm"));
        assertTrue(item.getJSONObject("preparation").getBoolean("manual_override"));
        assertEquals(128,item.getJSONObject("remote_preparation").getInt("bpm"));
        assertEquals("new-sha",item.getString("sha256"));
        assertEquals("",item.getJSONObject("preparation").optString("source_sha256"));
        assertTrue(item.getJSONObject("revision").has("old"));
    }
    @Test public void latePreparationMergesAgainstRetainedBaselineWithoutDiscardingManualEdits() throws Exception {
        JSONObject old = new JSONObject().put("preparation",new JSONObject().put("bpm",129).put("cue_in",2))
            .put("remote_preparation",new JSONObject().put("bpm",128).put("cue_in",2));
        JSONObject incoming = new JSONObject().put("preparation",new JSONObject().put("bpm",130).put("cue_in",3))
            .put("revision",new JSONObject().put("incoming",1));
        JSONArray conflicts=new JSONArray();JSONObject item=new JSONObject(old.toString());
        OfflineSongMetadata.mergePreparation(item,old,incoming,"sha","song-key",conflicts);
        assertEquals(129,item.getJSONObject("preparation").getInt("bpm"));
        assertEquals(3,item.getJSONObject("preparation").getInt("cue_in"));
        assertEquals(130,item.getJSONObject("remote_preparation").getInt("bpm"));
        assertEquals(1,conflicts.length());assertEquals("sha",item.getString("sha256"));
    }
    @Test public void basicOnlyTrackNeverInventsDjPreparation() throws Exception {
        JSONObject item=new JSONObject();OfflineSongMetadata.mergePreparation(item,null,null,"sha","song-key",new JSONArray());
        assertFalse(item.has("preparation"));assertFalse(item.has("remote_preparation"));
    }
}
