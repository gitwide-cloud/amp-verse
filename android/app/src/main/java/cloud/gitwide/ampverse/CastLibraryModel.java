package cloud.gitwide.ampverse;
import org.json.*;
import java.util.*;
/** Stable server references, independent of mutable catalog row indices. */
final class CastLibraryModel {
    static String path(String path)throws JSONException {
        if(path==null||path.isEmpty()||path.startsWith("/")||path.contains("\\")||path.contains(":")||path.indexOf(0)>=0)
            throw new JSONException("Invalid server relative path");
        for(String part:path.split("/",-1))if(part.isEmpty()||part.equals(".")||part.equals(".."))throw new JSONException("Invalid server relative path");
        return path;
    }
    static String catalogPath(String nativePath)throws JSONException {return path(nativePath.replace('\\','/'));}
    static void requireAccess(JSONObject status,String password)throws JSONException {if(status.optBoolean("password_required")&&password.isEmpty())throw new JSONException("This selected server requires its optional access code");}
    static boolean supports(JSONObject status){JSONObject caps=status.optJSONObject("capabilities");return caps!=null&&caps.optInt("player_queue")>=1&&"library_path_v1".equals(caps.optString("player_track_refs"));}
    static JSONObject play(String library,JSONArray refs,boolean speakers)throws JSONException {
        if(library.isEmpty()||refs.length()==0||refs.length()>20000)throw new JSONException("Choose available server songs");
        JSONArray paths=new JSONArray();for(int i=0;i<refs.length();i++)paths.put(path(refs.getString(i)));
        return new JSONObject().put("library_id",library).put("tracks",paths).put("shuffle",false).put("server_speakers",speakers);
    }
    static JSONObject savedPlay(String library,JSONObject envelope,boolean speakers)throws JSONException {
        String id=envelope.getString("document_id"),kind=envelope.getString("kind");
        if(library.isEmpty()||!(kind.equals("normal")&&id.startsWith("normal_")||kind.equals("smart")&&id.startsWith("smart_")))throw new JSONException("Invalid saved playlist identity");
        return new JSONObject().put("library_id",library).put("playlist_id",id).put("shuffle",false).put("server_speakers",speakers);
    }
    static final class Snapshot {final JSONArray paths;final int unavailable;final boolean smart;Snapshot(JSONArray p,int u,boolean s){paths=p;unavailable=u;smart=s;}}
    static Snapshot savedSnapshot(String library,JSONObject envelope,JSONObject snapshot)throws JSONException {
        boolean smart=envelope.getString("kind").equals("smart");String mode=smart?"eligible_snapshot":"ordered";
        if(!library.equals(snapshot.getString("library_id"))||!envelope.getString("document_id").equals(snapshot.getString("playlist_id"))||!mode.equals(snapshot.getString("selection_mode")))throw new JSONException("Saved playlist snapshot authority or mode changed");
        JSONArray tracks=snapshot.getJSONArray("tracks"),paths=new JSONArray();if(tracks.length()>20000)throw new JSONException("Saved playlist snapshot exceeds phone queue limit");
        for(int i=0;i<tracks.length();i++)paths.put(catalogPath(tracks.getJSONObject(i).getString("path")));
        int unavailable=snapshot.optInt("unavailable");if(unavailable<0)throw new JSONException("Invalid unavailable count");
        return new Snapshot(paths,unavailable,smart);
    }
    static JSONObject edited(JSONObject envelope,String name,String style,String energy)throws JSONException {
        if(name.trim().isEmpty()||name.getBytes(java.nio.charset.StandardCharsets.UTF_8).length>200)throw new JSONException("Enter a playlist name of 1–200 UTF-8 bytes");
        JSONObject document=new JSONObject(envelope.getJSONObject("document").toString()).put("name",name.trim());
        if(envelope.getString("kind").equals("smart")){
            if(!Arrays.asList("steady","hype","chill").contains(style))throw new JSONException("Unknown Smart style");
            document.put("style",style);if(energy.trim().isEmpty())document.put("target_energy",JSONObject.NULL);
            else{double v=Double.parseDouble(energy.trim());if(!Double.isFinite(v)||v<1||v>10)throw new JSONException("Energy must be 1–10");document.put("target_energy",v);}
        }
        return document;
    }
}
