package cloud.gitwide.ampverse;

import android.content.Context;
import android.net.Uri;
import androidx.media3.common.MediaItem;
import androidx.media3.common.MediaMetadata;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.File;
import java.nio.file.Files;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/** Local references stay in place. Only AMP playlist downloads enter managed sync storage. */
final class LibraryStore {
    static List<MediaItem> read(Context context) {
        List<MediaItem> items = new ArrayList<>();
        File index = new File(context.getFilesDir(), "ampdj/library.json");
        if(!index.exists())index=new File(context.getFilesDir(),"library.json");
        String packed=CatalogStore.get(context,"library");
        if (!index.exists()&&packed==null) return items;
        try {
            JSONArray array = new JSONArray(packed!=null?packed:new String(Files.readAllBytes(index.toPath()), StandardCharsets.UTF_8));
            for (int i = 0; i < array.length(); i++) {
                JSONObject entry = array.getJSONObject(i);
                if(entry.has("uri")) {
                    Uri uri=Uri.parse(entry.getString("uri"));
                    if(!"content".equals(uri.getScheme()))continue;
                    MediaMetadata.Builder metadata=new MediaMetadata.Builder().setTitle(entry.optString("title")).setArtist(entry.optString("artist")).setAlbumTitle(entry.optString("album"));
                    if(entry.has("preparation"))metadata.setExtras(DjEngine.preparedExtras(entry.getString("preparation")));
                    items.add(new MediaItem.Builder().setMediaId(entry.getString("id")).setUri(uri).setMediaMetadata(metadata.build()).build());
                } else {
                    File file = new File(context.getFilesDir(), entry.getString("file"));
                    if (!file.getCanonicalPath().startsWith(context.getFilesDir().getCanonicalPath() + File.separator)) continue;
                    if (file.isFile()) items.add(item(file, entry.getString("title")));
                }
            }
        } catch (Exception error) { throw new IllegalStateException("Cannot read saved library", error); }
        return items;
    }
    static MediaItem reference(Context context,Uri uri,String name,String preparation) {
        String title=name,artist="",album="";
        android.media.MediaMetadataRetriever tags=new android.media.MediaMetadataRetriever();
        try{tags.setDataSource(context,uri);String value=tags.extractMetadata(android.media.MediaMetadataRetriever.METADATA_KEY_TITLE);if(value!=null&&!value.isEmpty())title=value;value=tags.extractMetadata(android.media.MediaMetadataRetriever.METADATA_KEY_ARTIST);if(value!=null)artist=value;value=tags.extractMetadata(android.media.MediaMetadataRetriever.METADATA_KEY_ALBUM);if(value!=null)album=value;}catch(Exception ignored){}finally{try{tags.release();}catch(Exception ignored){}}
        MediaMetadata.Builder metadata=new MediaMetadata.Builder().setTitle(title).setArtist(artist).setAlbumTitle(album);
        if(preparation!=null)metadata.setExtras(DjEngine.preparedExtras(preparation));
        return new MediaItem.Builder().setMediaId("local-"+java.util.UUID.nameUUIDFromBytes(uri.toString().getBytes(StandardCharsets.UTF_8))).setUri(uri).setMediaMetadata(metadata.build()).build();
    }
    static MediaItem item(File file, String title) {
        MediaMetadata.Builder metadata=new MediaMetadata.Builder().setTitle(title).setArtist("Local audio");
        File prep=new File(file.getParentFile().getParentFile(),"ampdj/tracks/"+file.getName()+".json");
        String packedPrep=CatalogStore.get(new File(file.getParentFile().getParentFile(),"ampdj/catalog.ampdb"),"tracks/"+file.getName()+".json");
        if(prep.isFile()||packedPrep!=null)try{android.os.Bundle extras=new android.os.Bundle();extras.putString("amp-preparation",packedPrep!=null?packedPrep:new String(Files.readAllBytes(prep.toPath()),StandardCharsets.UTF_8));metadata.setExtras(extras);}catch(Exception error){throw new IllegalStateException("Cannot read prepared cues",error);}
        return new MediaItem.Builder().setMediaId(file.getName()).setUri(Uri.fromFile(file)).setMediaMetadata(metadata.build()).build();
    }
    static void write(Context context, List<MediaItem> items) throws Exception {
        JSONArray array = new JSONArray();
        for (MediaItem item : items) {
            if(item.localConfiguration==null)continue;
            Uri uri=item.localConfiguration.uri;
            if("content".equals(uri.getScheme())) {
                JSONObject entry=new JSONObject().put("uri",uri.toString()).put("id",item.mediaId).put("title",item.mediaMetadata.title).put("artist",item.mediaMetadata.artist==null?"":item.mediaMetadata.artist).put("album",item.mediaMetadata.albumTitle==null?"":item.mediaMetadata.albumTitle);
                if(item.mediaMetadata.extras!=null&&item.mediaMetadata.extras.containsKey("amp-preparation"))entry.put("preparation",item.mediaMetadata.extras.getString("amp-preparation"));
                array.put(entry);
            }else{
                File file = new File(uri.getPath());
                if(!file.getCanonicalPath().startsWith(new File(context.getFilesDir(),"audio").getCanonicalPath()+File.separator))continue;
                array.put(new JSONObject().put("file", "audio/" + file.getName()).put("title", item.mediaMetadata.title));
            }
        }
        CatalogStore.put(context,"library",array.toString());
    }
}
