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
    static File save(Context context,CastClient client,JSONObject manifest,JSONObject playlist)throws Exception {
        File base=new File(context.getFilesDir(),"radio-offline");if(!base.isDirectory()&&!base.mkdirs())throw new IOException("Cannot create offline storage");
        String token=UUID.randomUUID().toString();File stage=new File(base,".pending-"+token);if(!stage.mkdir())throw new IOException("Cannot stage download");boolean published=false;
        try {
            JSONArray ids=playlist.getJSONArray("tracks"),tracks=manifest.getJSONArray("tracks"),items=new JSONArray();Map<Integer,JSONObject> catalog=new HashMap<>();for(int i=0;i<tracks.length();i++){JSONObject t=tracks.getJSONObject(i);catalog.put(t.getInt("id"),t);}
            Map<Integer,JSONObject> copied=new HashMap<>();
            for(int i=0;i<ids.length();i++){
                int id=ids.getInt(i);JSONObject item=copied.get(id);
                if(item==null){
                    JSONObject entry=catalog.get(id);if(entry==null)throw new IOException("Song missing from station");
                    JSONObject meta=client.get("/api/smart/tracks/"+id+"/metadata");
                    for(String key:new String[]{"server_id","station_id","library_id"})if(!manifest.optString(key).equals(meta.optString(key)))throw new IOException("Station changed during download");
                    String extension=entry.optString("extension","audio");if(!extension.matches("[A-Za-z0-9]{1,8}"))extension="audio";
                    String name="track-"+id+"."+extension;File audio=new File(stage,name);
                    client.download(id,audio,meta.getLong("bytes"),meta.getString("sha256"));
                    JSONObject preparation=meta.getJSONObject("preparation");preparation.put("source_sha256",meta.getString("sha256"));
                    CatalogStore.put(new File(stage,"ampdj/catalog.ampdb"),"tracks/"+name+".json",preparation.toString());
                    item=new JSONObject().put("file",name).put("title",entry.optString("title","Song")).put("preparation",preparation);copied.put(id,item);
                }
                items.put(item);
            }
            JSONObject album=new JSONObject().put("version",1).put("name",playlist.optString("name","Station playlist")).put("source",manifest).put("playlist",playlist).put("items",items).put("offline_changes","local_only");
            CatalogStore.put(new File(stage,"ampdj/catalog.ampdb"),"offline-playlist",album.toString());
            File target=new File(base,token);Files.move(stage.toPath(),target.toPath(),StandardCopyOption.ATOMIC_MOVE);published=true;return target;
        } finally {if(!published)removeStage(stage);}
    }
    private static void removeStage(File file){File[] children=file.listFiles();if(children!=null)for(File child:children)removeStage(child);file.delete();}
    static List<File> albums(Context context){List<File> result=new ArrayList<>();File[] files=new File(context.getFilesDir(),"radio-offline").listFiles();if(files!=null)for(File file:files)if(!file.getName().startsWith(".")&&(new File(file,"playlist.json").isFile()||new File(file,"ampdj/catalog.ampdb").isFile()))result.add(file);return result;}
    static JSONObject album(File folder)throws Exception{String packed=CatalogStore.get(new File(folder,"ampdj/catalog.ampdb"),"offline-playlist");return new JSONObject(packed!=null?packed:new String(Files.readAllBytes(new File(folder,"playlist.json").toPath()),StandardCharsets.UTF_8));}
    static List<MediaItem> items(File folder)throws Exception {
        List<MediaItem> result=new ArrayList<>();JSONArray entries=album(folder).getJSONArray("items");
        for(int i=0;i<entries.length();i++){JSONObject entry=entries.getJSONObject(i);File audio=new File(folder,entry.getString("file"));if(!audio.getCanonicalPath().startsWith(folder.getCanonicalPath()+File.separator)||!audio.isFile())throw new IOException("Offline song unavailable");android.os.Bundle extras=new android.os.Bundle();extras.putString("amp-preparation",entry.getJSONObject("preparation").toString());result.add(new MediaItem.Builder().setMediaId("station-"+folder.getName()+"-"+audio.getName()).setUri(android.net.Uri.fromFile(audio)).setMediaMetadata(new MediaMetadata.Builder().setTitle(entry.optString("title")).setArtist("Offline station").setExtras(extras).build()).build());}return result;
    }
    static List<MediaItem> all(Context context){List<MediaItem> result=new ArrayList<>();for(File folder:albums(context))try{result.addAll(items(folder));}catch(Exception ignored){}return result;}
}
