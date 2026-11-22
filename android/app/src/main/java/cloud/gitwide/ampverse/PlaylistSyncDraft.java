package cloud.gitwide.ampverse;
import java.util.*;
import org.json.*;

/** Explicit publication of a COPY. Device paths/IDs are never guessed or uploaded. */
final class PlaylistSyncDraft {
    static JSONArray required(JSONObject local,boolean allLibrary)throws JSONException {
        LinkedHashSet<String> ids=new LinkedHashSet<>();
        if(!local.optBoolean("smart")||!allLibrary)add(ids,local.optJSONArray("songs"));
        add(ids,local.optJSONArray("excluded"));add(ids,local.optJSONArray("preferred"));
        String recognition=local.optString("recognition_song");if(!recognition.isEmpty())ids.add(recognition);
        JSONArray result=new JSONArray();for(String id:ids)result.put(id);return result;
    }
    private static void add(Set<String> ids,JSONArray values)throws JSONException {if(values!=null)for(int i=0;i<values.length();i++)ids.add(values.getString(i));}
    private static JSONObject reference(JSONObject state,JSONObject selected,String phone)throws JSONException {
        String key=selected.optString(phone);JSONObject known=state.getJSONObject("catalog").optJSONObject(key);
        if(known==null||!state.getString("library_id").equals(known.optString("library_id")))throw new JSONException("Explicitly match every phone song to this server's catalog before publishing");
        String path=known.getString("relative_path");if(!key.equals(PlaylistSyncModel.trackKey(path)))throw new JSONException("Catalog track identity changed; refresh before publishing");
        return new JSONObject().put("library_id",state.getString("library_id")).put("track_key",key).put("relative_path",path);
    }
    private static JSONArray keys(JSONObject state,JSONObject selected,JSONArray phones)throws JSONException {JSONArray keys=new JSONArray();if(phones!=null)for(int i=0;i<phones.length();i++)keys.put(reference(state,selected,phones.getString(i)).getString("track_key"));return keys;}
    static JSONObject prepare(JSONObject local,JSONObject state,JSONObject selected,boolean allLibrary,String id)throws JSONException {
        boolean smart=local.optBoolean("smart");JSONObject document=PlaylistSyncModel.copy(local);
        for(String field:new String[]{"id","smart","songs","source","sources","energy","source_root_hint","source_unavailable","source_library"})document.remove(field);
        document.put("name",local.getString("name")).put("companion_available",false);
        if(smart){
            document.put("id",id).put("style",local.optString("style","steady").toLowerCase(Locale.ROOT));
            document.put("folders",new JSONArray()).put("playlist_sources",new JSONArray()).put("restrict_folders",false);
            document.put("allowed",allLibrary?new JSONArray():keys(state,selected,local.optJSONArray("songs"))).put("restrict_allowed",!allLibrary);
            document.put("excluded",keys(state,selected,local.optJSONArray("excluded"))).put("preferred",keys(state,selected,local.optJSONArray("preferred")));
            String recognition=local.optString("recognition_song");document.put("recognition_song",recognition.isEmpty()?JSONObject.NULL:reference(state,selected,recognition).getString("track_key"));
            double energy=local.optDouble("energy",-1);document.put("target_energy",energy<0?JSONObject.NULL:1+9*energy);
            JSONObject progression=document.optJSONObject("energy_progression");if(progression!=null)progression.put("target",1+9*progression.getDouble("target"));
            document.put("fallback",local.optString("fallback","relax_recent")).put("recent_repeat",local.optInt("recent_repeat",5)).put("use_manual_cues",local.optBoolean("use_manual_cues",true));
        }else{
            JSONArray refs=new JSONArray(),phones=local.optJSONArray("songs");if(phones!=null)for(int i=0;i<phones.length();i++)refs.put(reference(state,selected,phones.getString(i)));
            document.put("version",1).put("tracks",refs);
        }
        return new JSONObject().put("document_id",(smart?"smart_":"normal_")+id).put("kind",smart?"smart":"normal").put("document_version",smart?2:1).put("new_document",true).put("revision",smart?state.opt("smart_revision"):JSONObject.NULL).put("document",document);
    }
}
