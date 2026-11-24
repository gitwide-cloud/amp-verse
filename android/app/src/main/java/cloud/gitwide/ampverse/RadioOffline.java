package cloud.gitwide.ampverse;
import android.content.Context;
import androidx.media3.common.*;
import java.io.*;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import org.json.*;
/** Separate station downloads. Never merges into the phone-folder library or uploads edits. */
final class RadioOffline {
    interface Progress {void song(int index,int total,String title);boolean cancelled();}
    static File save(Context context,CastClient client,JSONObject manifest,JSONObject playlist)throws Exception {
        return save(context,client,manifest,playlist,new Progress(){public void song(int i,int n,String t){}public boolean cancelled(){return Thread.currentThread().isInterrupted();}});
    }
    static boolean djAvailable(Context context) {
        if(!context.getSharedPreferences("modules",0).getBoolean("enabled-dj",true))return false;
        if(Modules.embedded("dj"))return true;
        try{Modules.installed(context,"dj");return true;}catch(Exception unavailable){return false;}
    }
    static File save(Context context,CastClient client,JSONObject manifest,JSONObject playlist,Progress progress)throws Exception {
        return save(context,client,manifest,playlist,progress,djAvailable(context));
    }
    /** Re-running with preparation=true catches up against existing audio without downloading it again. */
    static synchronized File save(Context context,CastClient client,JSONObject manifest,JSONObject playlist,Progress progress,boolean preparationRequested)throws Exception {
        File base=new File(context.getFilesDir(),"radio-offline");if(!base.isDirectory()&&!base.mkdirs())throw new IOException("Cannot create offline storage");
        File previousFolder=null;JSONObject previous=null;
        for(File folder:albums(context)){JSONObject candidate=album(folder);if(sameSource(candidate.optJSONObject("source"),manifest)&&playlistKey(candidate.getJSONObject("playlist")).equals(playlistKey(playlist))){previousFolder=folder;previous=candidate;break;}}
        PreviousSongs previousSongs=new PreviousSongs(previous);
        String token=previousFolder==null?UUID.randomUUID().toString():previousFolder.getName();
        File stage=new File(base,".pending-"+UUID.randomUUID());if(!stage.mkdir())throw new IOException("Cannot stage download");
        boolean published=false;List<android.net.Uri> stagedAudio=new ArrayList<>();List<File> stagedLegacy=new ArrayList<>();List<Runnable> cueMigrations=new ArrayList<>();int downloaded=0,reused=0;JSONArray conflicts=previous==null?new JSONArray():new JSONArray(previous.optJSONArray("conflicts")==null?"[]":previous.getJSONArray("conflicts").toString());
        try{
            JSONArray ids=playlist.getJSONArray("tracks"),tracks=manifest.getJSONArray("tracks"),items=new JSONArray();Map<Integer,JSONObject> catalog=new HashMap<>();for(int i=0;i<tracks.length();i++){JSONObject t=tracks.getJSONObject(i);catalog.put(t.getInt("id"),t);}
            if(ids.length()==0&&previous==null)throw new IOException("This playlist has no songs to download. Existing offline songs were kept.");
            Map<Integer,JSONObject> copied=new HashMap<>();
            for(int i=0;i<ids.length();i++){
                if(progress.cancelled())throw new IOException("Download cancelled. Existing offline playlists were kept.");
                int id=ids.getInt(i);JSONObject item=copied.get(id),entry=catalog.get(id);if(entry==null)throw new IOException("Song missing from station");progress.song(i+1,ids.length(),entry.optString("title","Song"));
                if(item==null){
                    JSONObject meta=metadata(client,id,"basic");if(!sameSource(meta,manifest))throw new IOException("Station changed during download");
                    String checksum=meta.getString("sha256").toLowerCase(java.util.Locale.ROOT);long bytes=meta.getLong("bytes");if(!checksum.matches("[A-Fa-f0-9]{64}")||bytes<0)throw new IOException("Invalid song integrity metadata");
                    String key=meta.optString("track_key",entry.optString("track_key",""));
                    if(!entry.optString("track_key").isEmpty()&&!entry.optString("track_key").equals(key))throw new IOException("Library changed during sync. Retry to refresh the playlist; your saved copy was kept.");
                    JSONObject old=previousSongs.find(key,checksum,entry.optString("title"));
                    String extension=entry.optString("extension","audio");if(!extension.matches("[A-Za-z0-9]{1,8}"))extension="audio";
                    String name="track-"+id+"-"+checksum.substring(0,16)+"-"+UUID.randomUUID().toString().substring(0,8)+"."+extension;
                    JSONObject preparationMetadata=null;
                    if(preparationRequested){
                        preparationMetadata=meta.has("preparation")?meta:metadata(client,id,"preparation");
                        if(!sameSource(preparationMetadata,manifest)||!key.equals(preparationMetadata.optString("track_key",key))||!checksum.equalsIgnoreCase(preparationMetadata.optString("sha256"))||bytes!=preparationMetadata.optLong("bytes",-1))throw new IOException("Song changed while fetching preparation. Your saved copy was kept.");
                    }
                    String mediaId=old==null?"station-"+token+"-"+name:old.optString("media_id","station-"+token+"-"+old.getString("file"));
                    if(old!=null&&checksum.equals(OfflineSongMetadata.checksum(old))&&available(context,previousFolder,old,bytes,checksum,progress)){
                        item=new JSONObject(old.toString());reused++;
                    }else{
                        File audio=new File(stage,name);client.download(id,audio,bytes,checksum,progress::cancelled);downloaded++;item=new JSONObject().put("file",name);
                        if(android.os.Build.VERSION.SDK_INT>=29){android.net.Uri managed=ManagedMusic.stage(context,audio,playlist.optString("name","Playlist"),entry.optString("title","Song"));stagedAudio.add(managed);audio.delete();item.put("uri",managed.toString());}
                        else if(previousFolder!=null){File target=new File(previousFolder,name);if(target.exists())throw new IOException("New download name collision; original kept");Files.move(audio.toPath(),target.toPath(),StandardCopyOption.ATOMIC_MOVE);stagedLegacy.add(target);}
                    }
                    item.put("title",meta.optString("title",entry.optString("title","Song"))).put("track_key",key).put("media_id",mediaId).put("bytes",bytes);
                    if(meta.optJSONObject("tags")!=null){
                        JSONObject remoteTags=new JSONObject(meta.getJSONObject("tags").toString()),localTags=old==null?null:old.optJSONObject("tags");
                        Object baseline=old==null?null:old.optJSONObject("remote_tags");if(baseline==null)baseline=localTags;
                        item.put("tags",localTags==null?remoteTags:SyncMerge.merge(baseline,localTags,remoteTags,"tags."+key,conflicts)).put("remote_tags",remoteTags);
                    }
                    OfflineSongMetadata.mergePreparation(item,old,preparationMetadata,checksum,key,conflicts);
                    if(old!=null&&preparationMetadata!=null)cueMigrations.add(PhoneCues.migration(context,mediaId,old.optJSONObject("preparation")==null?new JSONObject():old.getJSONObject("preparation"),item.getJSONObject("remote_preparation"),conflicts));
                    copied.put(id,item);
                }
                items.put(item);
            }
            if(progress.cancelled())throw new IOException("Download cancelled. Existing offline playlists were kept.");
            JSONObject settings=playlist;
            if(previous!=null){JSONObject local=new JSONObject(previous.getJSONObject("playlist").toString()),incoming=new JSONObject(playlist.toString()),baseline=previous.optJSONObject("remote_playlist");if(baseline==null)baseline=previous.getJSONObject("playlist");settings=(JSONObject)SyncMerge.merge(baseline,local,incoming,"playlist",conflicts);settings.put("tracks",playlist.getJSONArray("tracks"));}
            java.util.LinkedHashMap<String,JSONObject> uniqueConflicts=new java.util.LinkedHashMap<>();for(int n=0;n<conflicts.length();n++){JSONObject conflict=conflicts.getJSONObject(n);uniqueConflicts.put(conflict.optString("field")+":"+conflict.optString("media_id")+":"+conflict.optInt("slot",-1),conflict);}conflicts=new JSONArray();for(JSONObject conflict:uniqueConflicts.values())conflicts.put(conflict);
            JSONObject album=new JSONObject().put("version",2).put("name",settings.optString("name","Station playlist")).put("source",manifest).put("playlist",settings).put("remote_playlist",new JSONObject(playlist.toString())).put("items",items).put("retained_items",previousSongs.retained(items)).put("offline_changes","local_only").put("conflicts",conflicts).put("sync_stats",new JSONObject().put("downloaded",downloaded).put("reused",reused));
            for(android.net.Uri uri:stagedAudio){if(progress.cancelled())throw new IOException("Download cancelled");ManagedMusic.publish(context,uri);}
            if(progress.cancelled())throw new IOException("Download cancelled. Existing offline playlists were kept.");
            File target=previousFolder==null?new File(base,token):previousFolder;
            if(previousFolder==null){CatalogStore.put(new File(stage,"ampdj/catalog.ampdb"),"offline-playlist",album.toString());Files.move(stage.toPath(),target.toPath(),StandardCopyOption.ATOMIC_MOVE);}
            else CatalogStore.put(new File(target,"ampdj/catalog.ampdb"),"offline-playlist",album.toString());
            published=true;for(Runnable migration:cueMigrations)migration.run();return target;
        }finally{if(!published){for(android.net.Uri uri:stagedAudio)ManagedMusic.discard(context,uri);for(File file:stagedLegacy)file.delete();}if(stage.exists())removeStage(stage);}
    }
    private static JSONObject metadata(CastClient client,int id,String kind)throws Exception {
        try{return client.get("/api/smart/tracks/"+id+"/"+kind);}
        catch(CastClient.HttpException error){
            // Compatibility only: old servers have no split routes. Never hide an actual server/data error.
            if(error.code!=404)throw error;
            return client.get("/api/smart/tracks/"+id+"/metadata");
        }
    }
    private static boolean sameSource(JSONObject a,JSONObject b){if(a==null||b==null)return false;for(String key:new String[]{"server_id","station_id","library_id"})if(a.optString(key).isEmpty()||!a.optString(key).equals(b.optString(key)))return false;return true;}
    private static String playlistKey(JSONObject p){return p.optString("id").isEmpty()?"legacy:"+p.optString("name"):p.optString("id");}
    private static final class PreviousSongs {
        final Map<String,JSONObject> byKey=new HashMap<>(),bySha=new HashMap<>(),byTitle=new HashMap<>(),all=new java.util.LinkedHashMap<>();final Set<String> duplicateTitles=new HashSet<>(),duplicateHashes=new HashSet<>();final Set<JSONObject> used=java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<>());
        PreviousSongs(JSONObject album)throws JSONException{if(album==null)return;for(String section:new String[]{"retained_items","items"}){JSONArray items=album.optJSONArray(section);if(items==null)continue;for(int i=0;i<items.length();i++){JSONObject item=items.getJSONObject(i);String key=item.optString("track_key"),identity=key.isEmpty()?item.optString("media_id",item.optString("file")):key;all.put(identity,item);String sha=OfflineSongMetadata.checksum(item);JSONObject sameHash=sha.isEmpty()?null:bySha.put(sha,item);if(sameHash!=null&&!sameHash.optString("track_key").equals(key))duplicateHashes.add(sha);if(!key.isEmpty())byKey.put(key,item);else{String title=item.optString("title");if(byTitle.put(title,item)!=null)duplicateTitles.add(title);}}}}
        JSONObject find(String key,String sha,String title){JSONObject item=byKey.get(key);if(item==null&&!duplicateHashes.contains(sha))item=bySha.get(sha);if(item==null&&!duplicateTitles.contains(title))item=byTitle.get(title);if(item!=null)used.add(item);return item;}
        JSONArray retained(JSONArray current)throws JSONException{Map<String,JSONObject> keep=new java.util.LinkedHashMap<>(all);keep.values().removeIf(used::contains);for(int i=0;i<current.length();i++){JSONObject item=current.getJSONObject(i);String key=item.optString("track_key");keep.remove(key.isEmpty()?item.optString("media_id",item.optString("file")):key);}JSONArray result=new JSONArray();for(JSONObject item:keep.values())result.put(item);return result;}
    }
    private static boolean available(Context context,File folder,JSONObject item,long bytes,String checksum,Progress progress){
        try{InputStream stream;
            if(item.has("uri")){android.net.Uri uri=android.net.Uri.parse(item.getString("uri"));if(!"content".equals(uri.getScheme())||!"media".equals(uri.getAuthority()))return false;try(android.content.res.AssetFileDescriptor file=context.getContentResolver().openAssetFileDescriptor(uri,"r")){if(file==null||file.getLength()!=bytes)return false;}stream=context.getContentResolver().openInputStream(uri);}
            else{File file=new File(folder,item.getString("file"));if(!file.getCanonicalPath().startsWith(folder.getCanonicalPath()+File.separator)||!file.isFile()||file.length()!=bytes)return false;stream=new FileInputStream(file);}
            if(stream==null)return false;java.security.MessageDigest digest=java.security.MessageDigest.getInstance("SHA-256");long size=0;try(InputStream input=stream){byte[] buffer=new byte[65536];int n;while((n=input.read(buffer))!=-1){if(progress.cancelled())return false;size+=n;if(size>bytes)return false;digest.update(buffer,0,n);}}StringBuilder hash=new StringBuilder();for(byte value:digest.digest())hash.append(String.format(java.util.Locale.ROOT,"%02x",value&255));return size==bytes&&hash.toString().equalsIgnoreCase(checksum);
        }catch(Exception error){return false;}
    }
    static String summary(File folder)throws Exception{JSONObject album=album(folder),stats=album.optJSONObject("sync_stats");int count=album.optJSONArray("conflicts")==null?0:album.getJSONArray("conflicts").length();return (stats==null?"Saved offline":stats.optInt("downloaded")+" downloaded · "+stats.optInt("reused")+" reused")+(count==0?". Your cues and settings are kept.":". "+count+" conflicting changes kept for review; phone edits retained.");}
    static void reviewConflicts(android.app.Activity activity,File folder){try{JSONObject album=album(folder);JSONArray conflicts=album.optJSONArray("conflicts");if(conflicts==null||conflicts.length()==0){new android.app.AlertDialog.Builder(activity).setTitle("Playlist sync").setMessage(summary(folder)).setPositiveButton("Done",null).show();return;}StringBuilder text=new StringBuilder("Your phone edits are still active. Desktop proposals are saved separately.\n\n");boolean cues=false;for(int i=0;i<conflicts.length();i++){JSONObject conflict=conflicts.getJSONObject(i);cues|=conflict.optString("field").equals("hot_cue");text.append(conflict.optString("field")).append(conflict.has("slot")?" "+(conflict.optInt("slot")+1):"").append("\nPhone: ").append(conflict.opt("phone")).append("\nDesktop: ").append(conflict.opt("desktop")).append("\n\n");}
        android.app.AlertDialog.Builder dialog=new android.app.AlertDialog.Builder(activity).setTitle("Review sync changes").setMessage(text.toString()).setPositiveButton("Keep phone versions",(d,n)->{try{album.put("conflicts",new JSONArray());CatalogStore.put(new File(folder,"ampdj/catalog.ampdb"),"offline-playlist",album.toString());}catch(Exception e){android.widget.Toast.makeText(activity,e.getMessage(),android.widget.Toast.LENGTH_LONG).show();}}).setNegativeButton("Later",null);
        if(cues)dialog.setNeutralButton("Use desktop hot cues",(d,n)->{try{JSONArray keep=new JSONArray();android.content.SharedPreferences.Editor edit=activity.getSharedPreferences("phone-cues",0).edit();for(int i=0;i<conflicts.length();i++){JSONObject conflict=conflicts.getJSONObject(i);if(conflict.optString("field").equals("hot_cue")){String id=conflict.getString("media_id");int slot=conflict.getInt("slot");edit.remove(id+":"+slot).remove(id+":manual:"+slot);}else keep.put(conflict);}edit.commit();album.put("conflicts",keep);CatalogStore.put(new File(folder,"ampdj/catalog.ampdb"),"offline-playlist",album.toString());}catch(Exception e){android.widget.Toast.makeText(activity,e.getMessage(),android.widget.Toast.LENGTH_LONG).show();}});dialog.show();
    }catch(Exception e){android.widget.Toast.makeText(activity,e.getMessage(),android.widget.Toast.LENGTH_LONG).show();}}
    private static void removeStage(File file){File[] children=file.listFiles();if(children!=null)for(File child:children)removeStage(child);file.delete();}
    static List<File> albums(Context context){List<File> result=new ArrayList<>();File[] files=new File(context.getFilesDir(),"radio-offline").listFiles();if(files!=null)for(File file:files)if(!file.getName().startsWith(".")&&(new File(file,"playlist.json").isFile()||new File(file,"ampdj/catalog.ampdb").isFile()))result.add(file);return result;}
    static JSONObject album(File folder)throws Exception{String packed=CatalogStore.get(new File(folder,"ampdj/catalog.ampdb"),"offline-playlist");return new JSONObject(packed!=null?packed:new String(Files.readAllBytes(new File(folder,"playlist.json").toPath()),StandardCharsets.UTF_8));}
    static List<MediaItem> items(File folder)throws Exception {
        List<MediaItem> result=new ArrayList<>();JSONArray entries=album(folder).getJSONArray("items");
        for(int i=0;i<entries.length();i++){JSONObject entry=entries.getJSONObject(i);File audio=new File(folder,entry.getString("file"));android.net.Uri source;
            if(entry.has("uri")){source=android.net.Uri.parse(entry.getString("uri"));if(!"content".equals(source.getScheme())||!"media".equals(source.getAuthority()))throw new IOException("Invalid managed music reference");}
            else{if(!audio.getCanonicalPath().startsWith(folder.getCanonicalPath()+File.separator)||!audio.isFile())throw new IOException("Offline song unavailable");source=android.net.Uri.fromFile(audio);}
            JSONObject tags=entry.optJSONObject("tags");if(tags==null)tags=new JSONObject();
            android.os.Bundle extras=new android.os.Bundle();if(entry.optJSONObject("preparation")!=null)extras.putString("amp-preparation",entry.getJSONObject("preparation").toString());result.add(new MediaItem.Builder().setMediaId(entry.optString("media_id","station-"+folder.getName()+"-"+audio.getName())).setUri(source).setMediaMetadata(new MediaMetadata.Builder().setTitle(entry.optString("title")).setArtist(tags.optString("artist","Offline station")).setAlbumTitle(tags.optString("album","")).setExtras(extras).build()).build());}return result;
    }
    static List<MediaItem> all(Context context){List<MediaItem> result=new ArrayList<>();for(File folder:albums(context))try{result.addAll(items(folder));}catch(Exception ignored){}return result;}
}
