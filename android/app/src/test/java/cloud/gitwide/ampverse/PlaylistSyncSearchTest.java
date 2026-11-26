package cloud.gitwide.ampverse;
import org.json.*;
import org.junit.Test;
import java.io.IOException;
import java.util.*;
import java.util.concurrent.atomic.*;
import static org.junit.Assert.*;

public class PlaylistSyncSearchTest {
    private PlaylistSyncHttp.Reply identity(String server)throws Exception {return new PlaylistSyncHttp.Reply(200,new JSONObject().put("server_id",server).put("library_id","library"));}
    private PlaylistSyncHttp.Reply beyondCache()throws Exception {return new PlaylistSyncHttp.Reply(200,new JSONObject().put("total",1).put("offset",0).put("tracks",new JSONArray().put(new JSONObject().put("id",20573).put("path","Gospel/album/song-20573.mp3").put("title","Song outside initial cache"))));}
    @Test public void wholeServerQuerySelectsUncachedReferenceWithoutRaisingCacheCapOrLosingDuplicates()throws Exception {
        JSONObject state=PlaylistSyncModel.state("server","library","phone");
        for(int i=0;i<2000;i++){String path="music/song-"+i+".mp3",key=PlaylistSyncModel.trackKey(path);state.getJSONObject("catalog").put(key,new JSONObject().put("library_id","library").put("track_key",key).put("relative_path",path));}
        ArrayList<String> calls=new ArrayList<>();PlaylistSyncSearch.Cancel cancel=new PlaylistSyncSearch.Cancel();
        PlaylistSyncSearch.Result result=PlaylistSyncSearch.fetch(path->{calls.add("public:"+path);return identity("server");},path->{calls.add("private:"+path);return beyondCache();},()->true,cancel,"server","library","Gospel & album?");
        assertEquals(3,calls.size());assertEquals("public:/api/status",calls.get(0));assertTrue(calls.get(1).contains("q=Gospel+%26+album%3F&offset=0&limit=200"));assertEquals("public:/api/status",calls.get(2));
        String key=PlaylistSyncModel.trackKey("Gospel/album/song-20573.mp3");assertFalse(state.getJSONObject("catalog").has(key));
        assertNull(PlaylistSyncModel.next(state)); // Searching never queues publication.
        JSONObject exact=PlaylistSyncSearch.mergeChoice(state,result,key,"server","library",true,cancel);
        assertEquals(2001,state.getJSONObject("catalog").length());assertEquals(3,exact.length());assertEquals(key,exact.getString("track_key"));
        JSONObject local=new JSONObject().put("smart",false).put("name","Gospel").put("songs",new JSONArray().put("phone-file").put("phone-file"));
        JSONObject envelope=PlaylistSyncDraft.prepare(local,state,new JSONObject().put("phone-file",key),false,"gospel-copy");
        assertEquals(2,envelope.getJSONObject("document").getJSONArray("tracks").length());
        assertEquals("Gospel/album/song-20573.mp3",envelope.getJSONObject("document").getJSONArray("tracks").getJSONObject(1).getString("relative_path"));
        PlaylistSyncModel.save(state,envelope,envelope.getJSONObject("document"));assertEquals(key,PlaylistSyncModel.next(state).getJSONObject("document").getJSONArray("tracks").getJSONObject(0).getString("track_key"));
    }
    @Test public void changedPublicIdentityNeverCallsCredentialedSearch()throws Exception {
        AtomicInteger privateCalls=new AtomicInteger();
        try{PlaylistSyncSearch.fetch(path->identity("different-server"),path->{privateCalls.incrementAndGet();return beyondCache();},()->true,new PlaylistSyncSearch.Cancel(),"server","library","Gospel");fail();}catch(IOException expected){}
        assertEquals(0,privateCalls.get());
    }
    @Test public void cancellationAndStaleBindingCannotMergeOrPublish()throws Exception {
        JSONObject state=PlaylistSyncModel.state("server","library","phone");PlaylistSyncSearch.Cancel cancel=new PlaylistSyncSearch.Cancel();
        PlaylistSyncSearch.Result result=PlaylistSyncSearch.fetch(path->identity("server"),path->beyondCache(),()->true,cancel,"server","library","Gospel");String key=result.entries.keys().next();
        try{PlaylistSyncSearch.mergeChoice(state,result,key,"server","library",false,cancel);fail();}catch(IOException expected){}
        cancel.cancel();try{PlaylistSyncSearch.mergeChoice(state,result,key,"server","library",true,cancel);fail();}catch(IOException expected){}
        assertEquals(0,state.getJSONObject("catalog").length());assertNull(PlaylistSyncModel.next(state));
        AtomicBoolean current=new AtomicBoolean(true);
        try{PlaylistSyncSearch.fetch(path->identity("server"),path->{current.set(false);return beyondCache();},current::get,new PlaylistSyncSearch.Cancel(),"server","library","Gospel");fail();}catch(IOException expected){}
    }
    @Test public void networkFailureDoesNotTurnIntoEmptySuccessfulSearchOrPublish()throws Exception {
        JSONObject state=PlaylistSyncModel.state("server","library","phone");
        try{PlaylistSyncSearch.fetch(path->identity("server"),path->{throw new IOException("fixture offline");},()->true,new PlaylistSyncSearch.Cancel(),"server","library","Gospel");fail();}catch(IOException expected){assertEquals("fixture offline",expected.getMessage());}
        assertEquals(0,state.getJSONObject("catalog").length());assertNull(PlaylistSyncModel.next(state));
    }
}
