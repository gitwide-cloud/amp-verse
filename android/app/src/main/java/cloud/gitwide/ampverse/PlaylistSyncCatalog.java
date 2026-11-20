package cloud.gitwide.ampverse;

import java.io.IOException;
import java.util.Arrays;
import org.json.JSONArray;
import org.json.JSONObject;

/** A failed page never becomes a successful permanently loaded catalog. */
final class PlaylistSyncCatalog {
    interface Pages { PlaylistSyncHttp.Reply get(int offset) throws Exception; }
    static final class Result {
        final JSONObject entries;
        final boolean truncated;
        Result(JSONObject entries, boolean truncated) { this.entries=entries; this.truncated=truncated; }
    }

    static Result fetch(Pages source, String library) throws Exception {
        JSONObject catalog=new JSONObject();
        int expectedTotal=-1;
        for (int offset=0;offset<2000;offset+=200) {
            PlaylistSyncHttp.Reply reply=source.get(offset);
            if (reply.code!=200) throw new IOException("Server catalog page failed; prior cache retained (HTTP "+reply.code+")");
            JSONObject page=reply.body;
            int total=page.getInt("total");
            if (total<0 || page.getInt("offset")!=offset || expectedTotal>=0 && total!=expectedTotal)
                throw new IOException("Server catalog changed during paging; retry required");
            expectedTotal=total;
            JSONArray rows=page.getJSONArray("tracks");
            int expected=Math.min(200,Math.max(0,total-offset));
            if (rows.length()!=expected) throw new IOException("Incomplete server catalog page; retry required");
            for (int i=0;i<rows.length();i++) {
                JSONObject row=rows.getJSONObject(i);
                String path=row.getString("path").replace('\\','/');
                if (path.isEmpty() || path.startsWith("/") || path.contains(":") || path.indexOf(0)>=0
                        || Arrays.asList(path.split("/",-1)).contains("..")
                        || Arrays.asList(path.split("/",-1)).contains(".")
                        || Arrays.asList(path.split("/",-1)).contains(""))
                    throw new IOException("Invalid server catalog reference");
                String key=PlaylistSyncModel.trackKey(path);
                if (catalog.has(key)) throw new IOException("Duplicate server catalog reference");
                catalog.put(key,new JSONObject().put("track_key",key).put("relative_path",path)
                        .put("library_id",library).put("title",row.optString("title",path)));
            }
            if (offset+rows.length()>=total) return new Result(catalog,false);
        }
        return new Result(catalog,expectedTotal>2000);
    }
}
