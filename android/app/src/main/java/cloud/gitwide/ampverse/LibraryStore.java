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
        File index = new File(context.getFilesDir(), "library.json");
        if (!index.exists()) return items;
        try {
            JSONArray array = new JSONArray(new String(Files.readAllBytes(index.toPath()), StandardCharsets.UTF_8));
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
        return new MediaItem.Builder().setMediaId(file.getName()).setUri(Uri.fromFile(file))
                .setMediaMetadata(new MediaMetadata.Builder().setTitle(title).setArtist("Local audio").build()).build();
    }
    static void write(Context context, List<MediaItem> items) throws Exception {
        JSONArray array = new JSONArray();
        for (MediaItem item : items) {
            File file = new File(item.localConfiguration.uri.getPath());
            array.put(new JSONObject().put("file", "audio/" + file.getName()).put("title", item.mediaMetadata.title));
        }
        File pending = new File(context.getFilesDir(), "library.pending");
        Files.write(pending.toPath(), array.toString().getBytes(StandardCharsets.UTF_8));
        Files.move(pending.toPath(), new File(context.getFilesDir(), "library.json").toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING, java.nio.file.StandardCopyOption.ATOMIC_MOVE);
    }
}
