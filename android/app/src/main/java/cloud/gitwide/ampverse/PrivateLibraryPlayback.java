package cloud.gitwide.ampverse;
import android.net.Uri;
import androidx.media3.common.*;
import java.util.*;
import java.util.concurrent.*;
import org.json.*;
/** Private credentials stay in this process, never in MediaItems, URLs, or persisted queues. */
final class PrivateLibraryPlayback {
 static final class Binding {
  final String base,password,server,library;final List<MediaItem> items;final long created=android.os.SystemClock.elapsedRealtime();
  Binding(String base,String password,String server,String library,List<MediaItem> items){this.base=base;this.password=password;this.server=server;this.library=library;this.items=items;}
 }
 private static final Map<String,Binding> bindings=new LinkedHashMap<>();
 private static final Set<String> prepared=new HashSet<>();
 static synchronized Binding binding(MediaItem item){if(item==null||!item.mediaId.startsWith("amp-private:"))return null;return bindings.get(item.mediaId.substring(12));}
 static boolean owns(MediaItem item){return item!=null&&item.mediaId.startsWith("amp-private:");}
 static JSONObject prepare(JSONObject args)throws Exception {
  String password=args.getString("password");if(password.isEmpty())throw new IllegalArgumentException("Private library access is required");
  CastClient c=new CastClient(args.getString("base"),password);JSONObject status=c.connect();
  String server=args.getString("server_id"),library=args.getString("library_id");
  if(!server.equals(status.optString("server_id"))||!library.equals(status.optString("library_id")))throw new IllegalStateException("Selected server/library changed");
  JSONObject capabilities=status.optJSONObject("capabilities");if(capabilities==null||!"library_path_v1".equals(capabilities.optString("private_audio")))throw new IllegalStateException("Update the selected server for private library streaming");
  JSONArray paths=args.getJSONArray("tracks");CastLibraryModel.play(library,paths,false);
  String token=UUID.randomUUID().toString();List<MediaItem> items=new ArrayList<>();
  for(int i=0;i<paths.length();i++){String path=paths.getString(i);Uri uri=Uri.parse(c.base).buildUpon().path("/api/player/audio").appendQueryParameter("library_id",library).appendQueryParameter("path",path).build();items.add(new MediaItem.Builder().setMediaId("amp-private:"+token).setUri(uri).setMediaMetadata(new MediaMetadata.Builder().setTitle(path.substring(path.lastIndexOf('/')+1)).build()).build());}
  String first=items.get(0).localConfiguration.uri.toString();
  try(okhttp3.Response check=PrivateLibraryHttp.create(c.base,password,server,library,first).newCall(new okhttp3.Request.Builder().url(first).head().build()).execute()){
   if(check.code()!=200&&check.code()!=206)throw new java.io.IOException("First saved reference is unavailable; phone playback was kept");
  }
  synchronized(PrivateLibraryPlayback.class){if(bindings.size()>=16){String oldest=bindings.keySet().iterator().next();bindings.remove(oldest);prepared.remove(oldest);}bindings.put(token,new Binding(c.base,password,server,library,items));prepared.add(token);}
  return new JSONObject().put("token",token).put("count",items.size());
 }
 static synchronized List<MediaItem> take(String token){Binding b=bindings.get(token);if(b==null||!prepared.remove(token)||android.os.SystemClock.elapsedRealtime()-b.created>60000)throw new IllegalStateException("Reopen the selected server song before playing");return b.items;}
}
