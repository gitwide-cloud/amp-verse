package cloud.gitwide.ampverse;

import org.json.*;

/** Integrity belongs to the audio, not to optional DJ preparation. */
final class OfflineSongMetadata {
    private OfflineSongMetadata() {}
    static String checksum(JSONObject item) {
        String sha=item.optString("sha256");
        JSONObject prep=item.optJSONObject("preparation");
        return sha.isEmpty() && prep!=null ? prep.optString("source_sha256") : sha;
    }
    static void mergePreparation(JSONObject item, JSONObject old, JSONObject incoming,
                                 String checksum, String key, JSONArray conflicts) throws JSONException {
        item.put("sha256",checksum);
        if(incoming==null) {
            // A basic refresh must not replace a local edit or its three-way baseline.
            if(old!=null) for(String field:new String[]{"preparation","remote_preparation","revision"})
                if(old.has(field)) item.put(field,old.get(field));
            JSONObject prep=item.optJSONObject("preparation");
            if(prep!=null&&!prep.optString("source_sha256").isEmpty()&&!checksum.equals(prep.optString("source_sha256")))
                item.put("preparation",new JSONObject(prep.toString()).put("sync_source_changed",true));
            return;
        }
        JSONObject remote=new JSONObject(incoming.getJSONObject("preparation").toString());
        remote.put("source_sha256",checksum);
        if(!incoming.optBoolean("preparation_current",true)) remote.put("sync_source_changed",true);
        else remote.remove("sync_source_changed");
        JSONObject preparation=remote;
        JSONObject oldPrep=old==null?null:old.optJSONObject("preparation");
        if(oldPrep!=null) {
            JSONObject baseline=old.optJSONObject("remote_preparation");
            if(baseline==null) baseline=oldPrep;
            preparation=(JSONObject)SyncMerge.merge(baseline,oldPrep,remote,"preparation."+key,conflicts);
        }
        preparation.put("source_sha256",checksum);
        item.put("preparation",preparation).put("remote_preparation",remote);
        if(incoming.has("revision")) item.put("revision",incoming.get("revision"));
    }
}
