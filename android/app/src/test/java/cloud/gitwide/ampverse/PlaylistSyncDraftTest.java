package cloud.gitwide.ampverse;
import org.json.*;
import org.junit.Test;
import static org.junit.Assert.*;

public class PlaylistSyncDraftTest {
    private JSONObject state()throws Exception {
        JSONObject state=PlaylistSyncModel.state("server","library","phone");
        String path="music/song.mp3",key=PlaylistSyncModel.trackKey(path);
        state.getJSONObject("catalog").put(key,new JSONObject().put("library_id","library").put("track_key",key).put("relative_path",path).put("title","Identical title is not identity"));
        return state;
    }
    private JSONObject selected()throws Exception {return new JSONObject().put("phone-song",PlaylistSyncModel.trackKey("music/song.mp3"));}
    @Test public void normalPublicationRequiresExactMappingAndRetainsDuplicatesAndOriginal()throws Exception {
        JSONObject local=new JSONObject().put("id","gospel-phone").put("name","Gospel").put("smart",false).put("source","content-private").put("songs",new JSONArray().put("phone-song").put("phone-song"));
        String before=local.toString();
        try{PlaylistSyncDraft.prepare(local,state(),new JSONObject(),false,"copy");fail("Matching title must never invent identity");}catch(JSONException expected){}
        JSONObject envelope=PlaylistSyncDraft.prepare(local,state(),selected(),false,"copy"),document=envelope.getJSONObject("document");
        assertEquals(2,document.getJSONArray("tracks").length());assertTrue(PlaylistSyncModel.equal(document.getJSONArray("tracks").get(0),document.getJSONArray("tracks").get(1)));
        assertFalse(document.has("songs"));assertFalse(document.has("source"));assertEquals(before,local.toString());
        JSONObject state=state();PlaylistSyncModel.save(state,envelope,document);JSONObject operation=PlaylistSyncModel.next(state);
        assertEquals("create",operation.getString("action"));assertEquals("normal_copy",operation.getString("document_id"));
        assertTrue(PlaylistSyncModel.deliveryStatus(state).contains("waiting for server acknowledgment"));
        PlaylistSyncModel.reply(state,operation,409,new JSONObject().put("status","conflict"));
        assertTrue(PlaylistSyncModel.deliveryStatus(state).contains("NOT been delivered"));assertEquals(before,local.toString());
    }
    @Test public void smartPublicationPreservesNonemptyFiltersAndExplicitSourceChoice()throws Exception {
        JSONObject filter=new JSONObject().put("type","Number").put("field","Bpm").put("min",90).put("max",130);
        JSONObject local=new JSONObject().put("id","local-smart").put("smart",true).put("name","Phone Gospel Smart").put("style","Hype").put("energy",.5).put("source","folder:phone-only").put("songs",new JSONArray().put("phone-song")).put("genres",new JSONArray().put("Gospel")).put("filters",new JSONArray().put(filter)).put("future_setting",new JSONObject().put("retain",true));
        JSONObject selection=PlaylistSyncDraft.prepare(local,state(),selected(),false,"smart-copy").getJSONObject("document");
        assertTrue(selection.getBoolean("restrict_allowed"));assertEquals(1,selection.getJSONArray("allowed").length());assertEquals(1,selection.getJSONArray("filters").length());assertEquals("Gospel",selection.getJSONArray("genres").getString(0));assertEquals(5.5,selection.getDouble("target_energy"),0);assertTrue(selection.getJSONObject("future_setting").getBoolean("retain"));
        JSONObject all=PlaylistSyncDraft.prepare(local,state(),new JSONObject(),true,"all-copy").getJSONObject("document");
        assertFalse(all.getBoolean("restrict_allowed"));assertEquals(0,all.getJSONArray("allowed").length());assertTrue(PlaylistSyncModel.equal(selection.getJSONArray("filters"),all.getJSONArray("filters")));
        JSONObject wrong=state();wrong.getJSONObject("catalog").getJSONObject(PlaylistSyncModel.trackKey("music/song.mp3")).put("library_id","other");
        try{PlaylistSyncDraft.prepare(local,wrong,selected(),false,"bad");fail("Authority mismatch must not publish");}catch(JSONException expected){}
    }
}
