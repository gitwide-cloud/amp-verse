package cloud.gitwide.ampverse;

import java.util.*;
import org.json.*;

/** Playlist-only durable state machine. No Android, media, analysis, or network ownership. */
final class PlaylistSyncModel {
    static String trackKey(String path)throws JSONException {try{byte[] d=java.security.MessageDigest.getInstance("SHA-256").digest(path.getBytes(java.nio.charset.StandardCharsets.UTF_8));return String.format(java.util.Locale.ROOT,"song_%064x",new java.math.BigInteger(1,d));}catch(java.security.GeneralSecurityException e){throw new JSONException("SHA256 unavailable");}}
    static JSONObject copy(JSONObject value)throws JSONException{return new JSONObject(value.toString());}
    static JSONObject state(String server,String library,String device)throws JSONException{return new JSONObject().put("version",1).put("server_id",server).put("library_id",library).put("device_id",device).put("documents",new JSONObject()).put("mappings",new JSONObject()).put("catalog",new JSONObject());}
    static boolean equal(Object a,Object b)throws JSONException {
        if(a==null||a==JSONObject.NULL)return b==null||b==JSONObject.NULL;
        if(a instanceof JSONObject&&b instanceof JSONObject){JSONObject x=(JSONObject)a,y=(JSONObject)b;if(x.length()!=y.length())return false;Iterator<String> keys=x.keys();while(keys.hasNext()){String k=keys.next();if(!y.has(k)||!equal(x.get(k),y.get(k)))return false;}return true;}
        if(a instanceof JSONArray&&b instanceof JSONArray){JSONArray x=(JSONArray)a,y=(JSONArray)b;if(x.length()!=y.length())return false;for(int i=0;i<x.length();i++)if(!equal(x.get(i),y.get(i)))return false;return true;}
        if(a instanceof Number&&b instanceof Number)return new java.math.BigDecimal(a.toString()).compareTo(new java.math.BigDecimal(b.toString()))==0;
        return a.equals(b);
    }
    static void authority(JSONObject state,JSONObject message)throws JSONException {
        if(!state.getString("server_id").equals(message.getString("server_id"))||!state.getString("library_id").equals(message.getString("library_id")))throw new JSONException("Selected server/library changed; synchronization paused");
    }
    static void pull(JSONObject state,JSONObject snapshot)throws JSONException {
        authority(state,snapshot);if(snapshot.getInt("protocol")!=1)throw new JSONException("Unsupported playlist sync protocol");
        JSONArray docs=snapshot.getJSONArray("documents");if(docs.length()>1000)throw new JSONException("Too many server playlists");
        JSONObject entries=state.getJSONObject("documents");
        for(int i=0;i<docs.length();i++){JSONObject canonical=copy(docs.getJSONObject(i));String id=canonical.getString("document_id");String kind=canonical.getString("kind");
            if(!(kind.equals("normal")&&id.startsWith("normal_")||kind.equals("smart")&&id.startsWith("smart_")))throw new JSONException("Unsupported playlist document");
            if(kind.equals("normal")){JSONArray refs=canonical.getJSONObject("document").getJSONArray("tracks");if(refs.length()>20000)throw new JSONException("Too many ordered references");for(int n=0;n<refs.length();n++){JSONObject ref=refs.getJSONObject(n);String path=ref.getString("relative_path");if(!state.getString("library_id").equals(ref.getString("library_id"))||path.isEmpty()||path.startsWith("/")||path.contains("\\")||path.contains(":")||Arrays.asList(path.split("/",-1)).contains("..")||Arrays.asList(path.split("/",-1)).contains("")||!trackKey(path).equals(ref.getString("track_key")))throw new JSONException("Server track reference identity is invalid");JSONObject known=copy(ref).put("title",path);if(!state.getJSONObject("catalog").has(ref.getString("track_key")))state.getJSONObject("catalog").put(ref.getString("track_key"),known);}}
            JSONObject entry=entries.optJSONObject(id);if(entry==null){entry=new JSONObject();entries.put(id,entry);}
            entry.put("current",canonical);if(!entry.has("desired")&&!entry.has("pending")&&!entry.has("blocked"))entry.put("baseline",canonical);
        }
        // Missing entries are retained: this protocol has no deletion tombstones.
        state.put("smart_revision",snapshot.opt("smart_revision")).put("snapshot_token",snapshot.optString("snapshot_token"));
    }
    static JSONObject visible(JSONObject entry)throws JSONException {return copy(entry.has("desired")?entry.getJSONObject("desired"):entry.getJSONObject("baseline"));}
    private static JSONObject mergeDraft(JSONObject base,JSONObject current,JSONObject proposed)throws JSONException {
        JSONObject merged=copy(current);Set<String> keys=new HashSet<>();Iterator<String> a=base.keys(),b=proposed.keys();while(a.hasNext())keys.add(a.next());while(b.hasNext())keys.add(b.next());
        for(String key:keys){if(equal(base.opt(key),proposed.opt(key)))continue;if(!equal(current.opt(key),base.opt(key))&&!equal(current.opt(key),proposed.opt(key)))throw new JSONException("Both drafts changed "+key);if(proposed.has(key))merged.put(key,proposed.get(key));else merged.remove(key);}return merged;
    }
    private static JSONObject operation(JSONObject state,JSONObject desired,String operationId)throws JSONException {
        Object baseline=desired.opt("revision");return new JSONObject().put("protocol",1).put("operation_id",operationId).put("device_id",state.getString("device_id")).put("server_id",state.getString("server_id")).put("library_id",state.getString("library_id")).put("document_id",desired.getString("document_id")).put("kind",desired.getString("kind")).put("document_version",desired.getInt("document_version")).put("action",desired.optBoolean("new_document")?"create":"update").put("base_revision",baseline==null?JSONObject.NULL:baseline).put("document",copy(desired.getJSONObject("document")));
    }
    private static void wireBound(JSONObject operation)throws JSONException {if(operation.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8).length>1024*1024)throw new JSONException("This edit exceeds the server operation limit; reduce it before saving");}
    static void save(JSONObject state,JSONObject original,JSONObject document)throws JSONException {
        String name=document.optString("name");if(name.trim().isEmpty()||name.getBytes(java.nio.charset.StandardCharsets.UTF_8).length>200)throw new JSONException("Playlist names must be 1–200 UTF-8 bytes");
        if(original.getString("kind").equals("normal")&&document.getJSONArray("tracks").length()>20000)throw new JSONException("Too many playlist references");
        if(!original.optBoolean("new_document")&&equal(original.getJSONObject("document"),document))return;
        wireBound(operation(state,copy(original).put("document",copy(document)),"00000000-0000-0000-0000-000000000000"));
        String id=original.getString("document_id");JSONObject entries=state.getJSONObject("documents"),entry=entries.optJSONObject(id);
        if(entry==null){entry=new JSONObject();entries.put(id,entry);}
        JSONObject desired=copy(original);desired.put("document",copy(document));
        if(entry.has("desired")){
            JSONObject previous=entry.getJSONObject("desired");
            try{if(!equal(previous.opt("revision"),original.opt("revision")))throw new JSONException("The editor baseline changed");desired=copy(previous);desired.put("document",mergeDraft(original.getJSONObject("document"),previous.getJSONObject("document"),document));}
            catch(JSONException conflict){entry.put("other_local_draft",copy(previous)).put("blocked",new JSONObject().put("status","local_draft_conflict")).put("blocked_message","Overlapping saved phone drafts require review");entry.put("desired_base_document",copy(original.getJSONObject("document")));entry.put("desired",copy(original).put("document",copy(document)));return;}
        }else entry.put("desired_base_document",copy(original.getJSONObject("document")));
        entry.put("desired",desired);
    }
    static JSONObject next(JSONObject state)throws JSONException {
        JSONObject entries=state.getJSONObject("documents");List<String> ids=new ArrayList<>();Iterator<String> it=entries.keys();while(it.hasNext())ids.add(it.next());Collections.sort(ids);
        for(String id:ids){JSONObject entry=entries.getJSONObject(id);if(entry.has("blocked"))continue;
            if(entry.has("pending"))return copy(entry.getJSONObject("pending"));
            if(!entry.has("desired"))continue;
            JSONObject desired=entry.getJSONObject("desired");
            JSONObject op=operation(state,desired,UUID.randomUUID().toString());wireBound(op);
            entry.put("pending",op);return copy(op);
        }return null;
    }
    static void reply(JSONObject state,JSONObject operation,int status,JSONObject response)throws JSONException {
        JSONObject entry=state.getJSONObject("documents").getJSONObject(operation.getString("document_id"));
        if(!entry.has("pending")||!entry.getJSONObject("pending").getString("operation_id").equals(operation.getString("operation_id")))throw new JSONException("Operation acknowledgment did not match pending request");
        if(status==200&&response.optString("status").equals("applied")){
            authority(state,response);if(!response.getString("operation_id").equals(operation.getString("operation_id")))throw new JSONException("Wrong operation acknowledgment");
            JSONObject canonical=copy(response.getJSONObject("canonical"));
            JSONObject revision=canonical.optJSONObject("revision");
            Object ordinal=revision==null?null:revision.opt("revision");
            Object schema=canonical.opt("document_version");int version=canonical.optInt("document_version",-1),requested=operation.getInt("document_version");
            if(!(schema instanceof Number)||!schema.toString().matches("[0-9]+")||!canonical.optString("kind").equals(operation.getString("kind"))||version<requested||version>3
                ||operation.getString("kind").equals("normal")&&version!=1
                ||revision==null||!(ordinal instanceof Number)||!ordinal.toString().matches("[0-9]+")
                ||!state.getString("library_id").equals(revision.optString("library_id")))
                throw new JSONException("Canonical acknowledgment shape or library revision is invalid");
            if(!canonical.getString("document_id").equals(operation.getString("document_id"))||!equal(canonical.getJSONObject("document"),operation.getJSONObject("document")))throw new JSONException("Acknowledgment document differs from saved proposal");
            entry.put("baseline",canonical).put("current",canonical);entry.remove("pending");
            JSONObject desired=entry.optJSONObject("desired");
            if(desired!=null&&!entry.has("blocked")){if(equal(desired.getJSONObject("document"),operation.getJSONObject("document"))){entry.remove("desired");entry.remove("desired_base_document");}else {
                JSONObject base=entry.getJSONObject("desired_base_document"),proposed=desired.getJSONObject("document"),merged=copy(canonical.getJSONObject("document"));Set<String> keys=new HashSet<>();Iterator<String> a=base.keys(),b=proposed.keys();while(a.hasNext())keys.add(a.next());while(b.hasNext())keys.add(b.next());for(String key:keys)if(!equal(base.opt(key),proposed.opt(key))){if(proposed.has(key))merged.put(key,proposed.get(key));else merged.remove(key);}
                desired.put("document",merged).put("revision",canonical.get("revision"));desired.remove("new_document");entry.put("desired_base_document",copy(canonical.getJSONObject("document")));
            }}
            if(operation.getString("kind").equals("smart")){
                Object oldRevision=operation.opt("base_revision"),newRevision=canonical.get("revision");if(equal(state.opt("smart_revision"),oldRevision))state.put("smart_revision",newRevision);
                JSONObject entries=state.getJSONObject("documents");Iterator<String> ids=entries.keys();while(ids.hasNext()){String id=ids.next();if(id.equals(operation.getString("document_id")))continue;JSONObject other=entries.getJSONObject(id);for(String slot:new String[]{"baseline","current","desired"}){JSONObject value=other.optJSONObject(slot);if(value!=null&&value.optString("kind").equals("smart")&&equal(value.opt("revision"),oldRevision)&&!other.has("blocked")&&!other.has("pending"))value.put("revision",newRevision).put("document_version",canonical.getInt("document_version"));}}
            }
        }else if(status==409||status==400){entry.put("blocked",copy(response)).put("blocked_message",status==409?"Conflict / uncertain receipt. Your phone edit is retained.":"Server rejected this playlist. Your phone edit is retained.");}
        // Transport/5xx: the same persisted pending operation ID is retried; never issue a replacement.
    }
    static void useServer(JSONObject state,String id,JSONObject reviewed)throws JSONException {
        JSONObject entry=state.getJSONObject("documents").getJSONObject(id);if(!equal(entry.opt("current"),reviewed))throw new JSONException("The server version changed since review; reopen the review.");if(!entry.has("current"))throw new JSONException("Refresh the server version first");
        entry.put("baseline",copy(entry.getJSONObject("current")));entry.remove("desired");entry.remove("desired_base_document");entry.remove("other_local_draft");entry.remove("pending");entry.remove("blocked");entry.remove("blocked_message");
    }
    static void reviewKeepDraft(JSONObject state,String id,JSONObject reviewed)throws JSONException {
        JSONObject entry=state.getJSONObject("documents").getJSONObject(id);if(!equal(entry.opt("current"),reviewed))throw new JSONException("The server version changed since review; reopen the review.");if(!entry.has("current")||!entry.has("desired"))throw new JSONException("Both server version and phone draft are required");
        // Called only after explicit review. New operation uses the freshly reviewed canonical revision.
        JSONObject base=entry.getJSONObject("desired_base_document"),proposed=entry.getJSONObject("desired").getJSONObject("document"),current=copy(entry.getJSONObject("current").getJSONObject("document"));
        Set<String> keys=new HashSet<>();Iterator<String> b=base.keys(),p=proposed.keys();while(b.hasNext())keys.add(b.next());while(p.hasNext())keys.add(p.next());
        JSONObject merged=copy(current);for(String key:keys){Object before=base.opt(key),after=proposed.opt(key);if(equal(before,after))continue;if(!equal(current.opt(key),before)&&!equal(current.opt(key),after))throw new JSONException("Both versions changed "+key+". Keep this conflict for review, or use the server version.");if(proposed.has(key))merged.put(key,after);else merged.remove(key);}
        entry.getJSONObject("desired").put("document",merged).put("revision",entry.getJSONObject("current").get("revision")).put("document_version",entry.getJSONObject("current").getInt("document_version")).remove("new_document");entry.put("desired_base_document",current);
        entry.remove("pending");entry.remove("blocked");entry.remove("blocked_message");entry.remove("other_local_draft");
    }
    static JSONArray orderedReferences(JSONObject envelope)throws JSONException {return new JSONArray(envelope.getJSONObject("document").getJSONArray("tracks").toString());}
    static List<String> mappedNormal(JSONObject envelope,JSONObject mappings)throws JSONException {
        List<String> out=new ArrayList<>();JSONArray refs=orderedReferences(envelope);
        for(int i=0;i<refs.length();i++){String id=mappings.optString(refs.getJSONObject(i).getString("track_key"));if(!id.isEmpty())out.add(id);}
        return out;
    }
}
