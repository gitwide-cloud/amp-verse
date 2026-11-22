package cloud.gitwide.ampverse;
import java.io.*;
import java.net.*;
import java.util.*;
import org.json.*;

/** Bounded read-only search of the entire selected server, independent of phone cache size. */
final class PlaylistSyncSearch {
    interface Source {PlaylistSyncHttp.Reply get(String path)throws Exception;}
    interface Current {boolean selected()throws Exception;}
    static final class Cancel {
        private boolean cancelled;private HttpURLConnection connection;
        synchronized void cancel(){cancelled=true;if(connection!=null)connection.disconnect();}
        synchronized boolean cancelled(){return cancelled;}
        synchronized void attach(HttpURLConnection next)throws IOException {if(cancelled)throw new IOException("Search cancelled");connection=next;}
        synchronized void detach(HttpURLConnection previous){if(connection==previous)connection=null;}
        void check()throws IOException {if(cancelled())throw new IOException("Search cancelled");}
    }
    static final class Result {
        final JSONObject entries;final int total;
        Result(JSONObject entries,int total){this.entries=entries;this.total=total;}
    }
    private static void identity(PlaylistSyncHttp.Reply reply,String server,String library)throws Exception {
        if(reply.code!=200||!server.equals(reply.body.optString("server_id"))||!library.equals(reply.body.optString("library_id")))throw new IOException("Selected server identity changed; reconnect before searching");
    }
    static Result fetch(Source publicSource,Source privateSource,Current current,Cancel cancel,String server,String library,String query)throws Exception {
        cancel.check();if(!current.selected())throw new IOException("Selected server changed; reopen search");
        identity(publicSource.get("/api/status"),server,library);
        cancel.check();if(!current.selected())throw new IOException("Selected server changed; reopen search");
        if(query.length()>256)throw new IOException("Refine the search to at most 256 characters");
        PlaylistSyncHttp.Reply reply=privateSource.get("/api/studio/tracks?q="+URLEncoder.encode(query,"UTF-8")+"&offset=0&limit=200");
        cancel.check();if(reply.code!=200)throw new IOException("Server search failed (HTTP "+reply.code+"); no playlist was published");
        identity(publicSource.get("/api/status"),server,library);
        cancel.check();if(!current.selected())throw new IOException("Selected server changed; search results discarded");
        JSONObject page=reply.body;JSONArray rows=page.getJSONArray("tracks");int total=page.getInt("total");
        if(page.getInt("offset")!=0||total<0||rows.length()!=Math.min(200,total))throw new IOException("Invalid server search page");
        JSONObject entries=new JSONObject();
        for(int i=0;i<rows.length();i++){
            JSONObject row=rows.getJSONObject(i);String path=row.getString("path").replace('\\','/');
            List<String> parts=Arrays.asList(path.split("/",-1));
            if(path.isEmpty()||path.startsWith("/")||path.contains(":")||path.indexOf(0)>=0||parts.contains("..")||parts.contains(".")||parts.contains(""))throw new IOException("Invalid server search reference");
            String key=PlaylistSyncModel.trackKey(path);if(entries.has(key))throw new IOException("Duplicate server search reference");
            entries.put(key,new JSONObject().put("library_id",library).put("track_key",key).put("relative_path",path).put("title",row.optString("title",path)));
        }
        return new Result(entries,total);
    }
    static JSONObject mergeChoice(JSONObject state,Result result,String key,String server,String library,boolean current,Cancel cancel)throws Exception {
        cancel.check();if(!current||!server.equals(state.getString("server_id"))||!library.equals(state.getString("library_id")))throw new IOException("Selected server changed; no search reference was saved");
        JSONObject ref=result.entries.getJSONObject(key);
        if(!library.equals(ref.getString("library_id"))||!key.equals(PlaylistSyncModel.trackKey(ref.getString("relative_path"))))throw new IOException("Search reference identity mismatch");
        state.getJSONObject("catalog").put(key,PlaylistSyncModel.copy(ref));
        return new JSONObject().put("library_id",library).put("track_key",key).put("relative_path",ref.getString("relative_path"));
    }
}
