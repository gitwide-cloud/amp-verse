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

/** Only locally copied audio enters this queue. Atomic metadata replacement, no network. */
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
                File file = new File(context.getFilesDir(), entry.getString("file"));
                if (!file.getCanonicalPath().startsWith(context.getFilesDir().getCanonicalPath() + File.separator)) continue;
                if (file.isFile()) items.add(item(file, entry.getString("title")));
            }
        } catch (Exception error) { throw new IllegalStateException("Cannot read saved library", error); }
        return items;
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
            File file = new File(item.localConfiguration.uri.getPath());
            array.put(new JSONObject().put("file", "audio/" + file.getName()).put("title", item.mediaMetadata.title));
        }
        CatalogStore.put(context,"library",array.toString());
    }
}
