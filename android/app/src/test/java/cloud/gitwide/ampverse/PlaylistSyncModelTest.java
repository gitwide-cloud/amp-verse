package cloud.gitwide.ampverse;

import org.junit.Test;
import static org.junit.Assert.*;
import org.json.*;
import java.util.*;

public class PlaylistSyncModelTest {
    JSONObject state()throws Exception{return PlaylistSyncModel.state("server","library","phone");}
    JSONObject normal(String name,int revision)throws Exception{return new JSONObject().put("document_id","normal_stable").put("kind","normal").put("document_version",1).put("revision",new JSONObject().put("revision",revision)).put("document",new JSONObject().put("version",1).put("name",name).put("companion_available",true).put("future",new JSONObject().put("keep",true)).put("tracks",new JSONArray().put(ref("a")).put(ref("missing")).put(ref("a"))));}
    JSONObject ref(String key)throws Exception{return new JSONObject().put("track_key",PlaylistSyncModel.trackKey(key+".mp3")).put("library_id","library").put("relative_path",key+".mp3");}
    JSONObject snapshot(JSONObject doc)throws Exception{return new JSONObject().put("protocol",1).put("server_id","server").put("library_id","library").put("smart_revision",new JSONObject().put("revision",1)).put("documents",new JSONArray().put(doc));}
    JSONObject ack(JSONObject op,int revision)throws Exception{return new JSONObject().put("server_id","server").put("library_id","library").put("operation_id",op.getString("operation_id")).put("status","applied").put("canonical",new JSONObject().put("document_id",op.getString("document_id")).put("kind",op.getString("kind")).put("document_version",1).put("revision",new JSONObject().put("revision",revision)).put("document",PlaylistSyncModel.copy(op.getJSONObject("document"))));}
    @Test public void pullPreservesDuplicatesAndUnresolvedMappingWithoutTouchingMedia()throws Exception{
        JSONObject s=state(),remote=normal("Server",1);PlaylistSyncModel.pull(s,snapshot(remote));JSONObject view=PlaylistSyncModel.visible(s.getJSONObject("documents").getJSONObject("normal_stable"));
        assertEquals(3,PlaylistSyncModel.orderedReferences(view).length());JSONObject mappings=new JSONObject().put(PlaylistSyncModel.trackKey("a.mp3"),"local-content-id");assertEquals(Arrays.asList("local-content-id","local-content-id"),PlaylistSyncModel.mappedNormal(view,mappings));assertEquals(3,remote.getJSONObject("document").getJSONArray("tracks").length());assertFalse(s.has("playback"));assertFalse(s.has("preparation"));
    }
    @Test public void openDraftKeepsCapturedBaselineAndPollingDoesNotOverwriteSavedDesired()throws Exception{
        JSONObject s=state();PlaylistSyncModel.pull(s,snapshot(normal("Old",1)));JSONObject draft=PlaylistSyncModel.visible(s.getJSONObject("documents").getJSONObject("normal_stable"));
        PlaylistSyncModel.pull(s,snapshot(normal("Desktop new",2)));assertEquals("Old",draft.getJSONObject("document").getString("name"));
        JSONObject desired=PlaylistSyncModel.copy(draft.getJSONObject("document")).put("name","Phone edit");PlaylistSyncModel.save(s,draft,desired);PlaylistSyncModel.pull(s,snapshot(normal("Desktop later",3)));
        JSONObject op=PlaylistSyncModel.next(s);assertEquals(1,op.getJSONObject("base_revision").getInt("revision"));assertEquals("Phone edit",op.getJSONObject("document").getString("name"));assertEquals("Desktop later",s.getJSONObject("documents").getJSONObject("normal_stable").getJSONObject("current").getJSONObject("document").getString("name"));
    }
    @Test public void offlineRestartRetriesExactOperationAndAckCannotEraseNewerSavedEdit()throws Exception{
        JSONObject s=state();JSONObject original=normal("Old",1);PlaylistSyncModel.pull(s,snapshot(original));PlaylistSyncModel.save(s,original,PlaylistSyncModel.copy(original.getJSONObject("document")).put("name","First"));
        JSONObject op=PlaylistSyncModel.next(s);s=new JSONObject(s.toString());assertTrue(PlaylistSyncModel.equal(op,PlaylistSyncModel.next(s)));
        PlaylistSyncModel.save(s,original,PlaylistSyncModel.copy(original.getJSONObject("document")).put("name","Newer"));PlaylistSyncModel.reply(s,op,200,ack(op,2));
        JSONObject next=PlaylistSyncModel.next(s);assertNotEquals(op.getString("operation_id"),next.getString("operation_id"));assertEquals(2,next.getJSONObject("base_revision").getInt("revision"));assertEquals("Newer",next.getJSONObject("document").getString("name"));assertTrue(next.getJSONObject("document").getJSONObject("future").getBoolean("keep"));assertTrue(next.getJSONObject("document").getBoolean("companion_available"));
    }
    @Test public void uncertainConflictNeverAutomaticallyReissuesPublication()throws Exception{
        JSONObject s=state(),original=normal("Old",1);PlaylistSyncModel.pull(s,snapshot(original));PlaylistSyncModel.save(s,original,PlaylistSyncModel.copy(original.getJSONObject("document")).put("name","Phone"));JSONObject op=PlaylistSyncModel.next(s);
        PlaylistSyncModel.reply(s,op,409,new JSONObject().put("status","uncertain"));s=new JSONObject(s.toString());assertNull(PlaylistSyncModel.next(s));assertEquals("Phone",PlaylistSyncModel.visible(s.getJSONObject("documents").getJSONObject("normal_stable")).getJSONObject("document").getString("name"));
        JSONObject desktop=normal("Old",3);desktop.getJSONObject("document").getJSONObject("future").put("desktop_extension",7);PlaylistSyncModel.pull(s,snapshot(desktop));PlaylistSyncModel.reviewKeepDraft(s,"normal_stable");JSONObject reviewed=PlaylistSyncModel.next(s);assertEquals(3,reviewed.getJSONObject("base_revision").getInt("revision"));assertNotEquals(op.getString("operation_id"),reviewed.getString("operation_id"));
    }
    @Test public void otherAuthorityAndWrongAckCannotReplaceBaselines()throws Exception{
        JSONObject s=state(),original=normal("Old",1);PlaylistSyncModel.pull(s,snapshot(original));JSONObject before=PlaylistSyncModel.copy(s);try{PlaylistSyncModel.pull(s,snapshot(normal("Other",2)).put("library_id","different"));fail();}catch(JSONException expected){}assertTrue(PlaylistSyncModel.equal(before,s));
        PlaylistSyncModel.save(s,original,PlaylistSyncModel.copy(original.getJSONObject("document")).put("name","Phone"));JSONObject op=PlaylistSyncModel.next(s);try{PlaylistSyncModel.reply(s,op,200,ack(op,2).put("operation_id","other"));fail();}catch(JSONException expected){}assertNotNull(PlaylistSyncModel.next(s));
    }
    @Test public void absentRemoteDocumentIsNotADeletionAndMappingsStayDeviceLocal()throws Exception{
        JSONObject s=state();s.getJSONObject("mappings").put(PlaylistSyncModel.trackKey("a.mp3"),"content-local");PlaylistSyncModel.pull(s,snapshot(normal("Old",1)));PlaylistSyncModel.pull(s,snapshot(normal("Old",1)).put("documents",new JSONArray()));
        assertTrue(s.getJSONObject("documents").has("normal_stable"));JSONObject original=normal("Old",1);PlaylistSyncModel.save(s,original,original.getJSONObject("document"));JSONObject op=PlaylistSyncModel.next(s);assertFalse(op.has("mappings"));assertFalse(op.toString().contains("content-local"));
    }

    @Test public void sameFieldConflictCannotBeBlindlyRebased()throws Exception{
        JSONObject s=state(),original=normal("Old",1);PlaylistSyncModel.pull(s,snapshot(original));PlaylistSyncModel.save(s,original,PlaylistSyncModel.copy(original.getJSONObject("document")).put("name","Phone"));JSONObject op=PlaylistSyncModel.next(s);PlaylistSyncModel.reply(s,op,409,new JSONObject().put("status","uncertain"));PlaylistSyncModel.pull(s,snapshot(normal("Desktop",2)));
        try{PlaylistSyncModel.reviewKeepDraft(s,"normal_stable");fail();}catch(JSONException expected){}
        assertNull(PlaylistSyncModel.next(s));assertEquals("Phone",PlaylistSyncModel.visible(s.getJSONObject("documents").getJSONObject("normal_stable")).getJSONObject("document").getString("name"));
    }
    @Test public void numericAckAndFullSmartRuleExtensionsRoundtrip()throws Exception{
        JSONObject s=state();JSONObject original=normal("Smart",1).put("document_id","smart_saved").put("kind","smart");JSONObject rules=original.getJSONObject("document");rules.remove("tracks");rules.put("id","saved").put("style","steady").put("target_energy",1.0).put("filters",new JSONArray().put(new JSONObject().put("type","Flag").put("field","Analyzed").put("value",true)));
        PlaylistSyncModel.pull(s,snapshot(original));PlaylistSyncModel.save(s,original,PlaylistSyncModel.copy(rules).put("name","Phone smart"));JSONObject op=PlaylistSyncModel.next(s),result=ack(op,2);result.getJSONObject("canonical").getJSONObject("document").put("target_energy",1);
        PlaylistSyncModel.reply(s,op,200,result);assertNull(PlaylistSyncModel.next(s));JSONObject saved=PlaylistSyncModel.visible(s.getJSONObject("documents").getJSONObject("smart_saved")).getJSONObject("document");assertEquals(1,saved.getJSONArray("filters").length());assertTrue(saved.getJSONObject("future").getBoolean("keep"));
    }

    @Test public void newerSavedDraftPatchesOwnAckWithoutRevertingOtherOwnFields()throws Exception{
        JSONObject s=state(),original=normal("Old",1);PlaylistSyncModel.pull(s,snapshot(original));JSONObject first=PlaylistSyncModel.copy(original.getJSONObject("document"));first.put("tracks",new JSONArray().put(ref("missing")));PlaylistSyncModel.save(s,original,first);JSONObject operation=PlaylistSyncModel.next(s);
        PlaylistSyncModel.save(s,original,PlaylistSyncModel.copy(original.getJSONObject("document")).put("name","Later name"));PlaylistSyncModel.reply(s,operation,200,ack(operation,2));JSONObject next=PlaylistSyncModel.next(s);
        assertEquals("Later name",next.getJSONObject("document").getString("name"));assertEquals(1,next.getJSONObject("document").getJSONArray("tracks").length());assertEquals(PlaylistSyncModel.trackKey("missing.mp3"),next.getJSONObject("document").getJSONArray("tracks").getJSONObject(0).getString("track_key"));
    }
    @Test public void ownSmartAckAdvancesOnlyMatchingQueuedContainerBaselines()throws Exception{
        JSONObject s=state(),a=normal("A",1).put("kind","smart").put("document_id","smart_a"),b=normal("B",1).put("kind","smart").put("document_id","smart_b");JSONObject snap=snapshot(a).put("documents",new JSONArray().put(a).put(b));PlaylistSyncModel.pull(s,snap);
        PlaylistSyncModel.save(s,a,PlaylistSyncModel.copy(a.getJSONObject("document")).put("name","A phone"));PlaylistSyncModel.save(s,b,PlaylistSyncModel.copy(b.getJSONObject("document")).put("name","B phone"));JSONObject first=PlaylistSyncModel.next(s);PlaylistSyncModel.reply(s,first,200,ack(first,2));JSONObject second=PlaylistSyncModel.next(s);assertEquals("smart_b",second.getString("document_id"));assertEquals(2,second.getJSONObject("base_revision").getInt("revision"));
    }

}

