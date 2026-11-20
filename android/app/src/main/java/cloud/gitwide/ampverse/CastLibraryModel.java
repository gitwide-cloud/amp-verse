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
    static boolean supports(JSONObject status){JSONObject caps=status.optJSONObject("capabilities");return caps!=null&&caps.optInt("player_queue")>=1&&"library_path_v1".equals(caps.optString("player_track_refs"));}
    static JSONObject play(String library,JSONArray refs,boolean speakers)throws JSONException {
        if(library.isEmpty()||refs.length()==0||refs.length()>20000)throw new JSONException("Choose available server songs");
        JSONArray paths=new JSONArray();for(int i=0;i<refs.length();i++)paths.put(path(refs.getString(i)));
        return new JSONObject().put("library_id",library).put("tracks",paths).put("shuffle",false).put("server_speakers",speakers);
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
