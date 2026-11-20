package cloud.gitwide.ampverse;

import org.junit.Test;
import static org.junit.Assert.*;
import java.io.IOException;
import java.util.concurrent.atomic.AtomicInteger;
import org.json.*;

public class PlaylistSyncCatalogTest {
    private PlaylistSyncHttp.Reply page(int total,int offset)throws Exception {
        JSONArray rows=new JSONArray();
        for(int i=offset;i<Math.min(offset+200,total);i++)rows.put(new JSONObject().put("path","music/song-"+i+".mp3").put("title","Song "+i));
        return new PlaylistSyncHttp.Reply(200,new JSONObject().put("total",total).put("offset",offset).put("tracks",rows));
    }

    @Test public void transientFirstPageFailureRetainsPriorCacheAndNextAttemptSucceeds()throws Exception {
        JSONObject old=new JSONObject().put("prior","kept");
        AtomicInteger attempts=new AtomicInteger();
        try{PlaylistSyncCatalog.fetch(offset->{attempts.incrementAndGet();return new PlaylistSyncHttp.Reply(503,new JSONObject());},"library");fail();}
        catch(IOException expected){assertEquals("kept",old.getString("prior"));}
        PlaylistSyncCatalog.Result retried=PlaylistSyncCatalog.fetch(offset->{attempts.incrementAndGet();return page(1,offset);},"library");
        assertEquals(2,attempts.get());assertEquals(1,retried.entries.length());assertFalse(retried.truncated);
    }

    @Test public void failedLaterPageCannotCommitAPartialCatalog()throws Exception {
        AtomicInteger calls=new AtomicInteger();
        try{PlaylistSyncCatalog.fetch(offset->{calls.incrementAndGet();return offset==0?page(201,0):new PlaylistSyncHttp.Reply(503,new JSONObject());},"library");fail();}
        catch(IOException expected){assertEquals(2,calls.get());}
        PlaylistSyncCatalog.Result retry=PlaylistSyncCatalog.fetch(offset->page(201,offset),"library");
        assertEquals(201,retry.entries.length());assertFalse(retry.truncated);
    }

    @Test public void intentionalBoundIsSuccessfulButMalformedOrChangingPagesAreNot()throws Exception {
        AtomicInteger pages=new AtomicInteger();
        PlaylistSyncCatalog.Result bounded=PlaylistSyncCatalog.fetch(offset->{pages.incrementAndGet();return page(3000,offset);},"library");
        assertEquals(10,pages.get());assertEquals(2000,bounded.entries.length());assertTrue(bounded.truncated);
        try{PlaylistSyncCatalog.fetch(offset->new PlaylistSyncHttp.Reply(200,new JSONObject().put("offset",offset).put("total",2).put("tracks",new JSONArray())),"library");fail();}catch(IOException expected){}
        try{PlaylistSyncCatalog.fetch(offset->page(offset==0?201:202,offset),"library");fail();}catch(IOException expected){}
    }
}
