package cloud.gitwide.ampverse;
import android.content.Context;
import org.json.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;

/** Network/file work stays off Binder and audio threads. Existing sync owns publication. */
final class ModuleJobs {
    static final class Job {final AtomicBoolean cancel=new AtomicBoolean();volatile String status="Starting…",error="";volatile boolean done;}
    private static final Map<String,Job> jobs=new ConcurrentHashMap<>();
    private static final ExecutorService worker=Executors.newSingleThreadExecutor();
    static JSONObject syncCast(Context context,JSONObject args)throws Exception{
        if(jobs.values().stream().anyMatch(job->!job.done))throw new IllegalStateException("A playlist download is already running");
        String id=UUID.randomUUID().toString();Job job=new Job();jobs.put(id,job);
        String address=args.getString("address"),password=args.optString("password"),playlistId=args.getString("playlist");
        worker.execute(()->{try{CastClient client=new CastClient(address,password);client.connect();JSONObject manifest=client.get("/api/smart/manifest");JSONArray lists=manifest.getJSONArray("playlists");JSONObject selected=null;for(int i=0;i<lists.length();i++)if(lists.getJSONObject(i).optString("id").equals(playlistId))selected=lists.getJSONObject(i);if(selected==null)throw new IllegalStateException("Playlist is no longer shared by this station");RadioOffline.save(context,client,manifest,selected,new RadioOffline.Progress(){public boolean cancelled(){return job.cancel.get();}public void song(int index,int total,String title){job.status=index+" / "+total+" · "+title;}});job.status="Available offline with prepared cues";}catch(Exception e){job.error=e.getMessage();job.status="Download stopped; previous offline playlist was retained";}finally{job.done=true;}});
        return new JSONObject().put("job",id);
    }
    static JSONObject state(String id,boolean cancel)throws Exception{Job job=jobs.get(id);if(job==null)throw new IllegalArgumentException("Download is no longer active. Check offline playlists.");if(cancel)job.cancel.set(true);return new JSONObject().put("job",id).put("done",job.done).put("status",job.status).put("error",job.error);}
    static synchronized JSONArray podcastDownloads(Context c)throws Exception{return new JSONArray(c.getSharedPreferences("modules",0).getString("podcasts.downloads","[]"));}
    static synchronized void addPodcast(Context c,JSONObject entry)throws Exception{JSONArray entries=podcastDownloads(c);entries.put(entry);if(!c.getSharedPreferences("modules",0).edit().putString("podcasts.downloads",entries.toString()).commit())throw new java.io.IOException("Cannot save download index");}
    static JSONObject podcast(Context context,JSONObject args)throws Exception{
        String id=UUID.randomUUID().toString(),url=args.getString("url"),title=args.optString("title","Podcast episode");
        java.net.URI uri=new java.net.URI(url);if(!Arrays.asList("http","https").contains(uri.getScheme())||uri.getHost()==null||uri.getUserInfo()!=null)throw new IllegalArgumentException("Unsupported episode URL");
        Job job=new Job();jobs.put(id,job);worker.execute(()->{java.io.File partial=null;java.net.HttpURLConnection connection=null;try{
            java.io.File root=new java.io.File(context.getFilesDir(),"podcasts/downloads");if(!root.isDirectory()&&!root.mkdirs())throw new java.io.IOException("Cannot create podcast download folder");partial=new java.io.File(root,id+".part");
            String target=url;for(int i=0;i<5;i++){connection=(java.net.HttpURLConnection)new java.net.URL(target).openConnection();connection.setInstanceFollowRedirects(false);connection.setConnectTimeout(10000);connection.setReadTimeout(10000);int code=connection.getResponseCode();if(code>=300&&code<400){String next=new java.net.URL(new java.net.URL(target),connection.getHeaderField("Location")).toString();connection.disconnect();if(!next.startsWith("https://")&&!next.startsWith("http://"))throw new java.io.IOException("Unsupported episode redirect");if(target.startsWith("https://")&&!next.startsWith("https://"))throw new java.io.IOException("Episode redirected to an insecure connection");target=next;continue;}if(code!=200)throw new java.io.IOException("Episode server returned "+code);break;}
            if(connection==null||connection.getResponseCode()!=200)throw new java.io.IOException("Too many episode redirects");long total=connection.getContentLengthLong(),count=0;
            try(java.io.InputStream in=connection.getInputStream();java.io.FileOutputStream out=new java.io.FileOutputStream(partial)){byte[] buffer=new byte[65536];int n;while((n=in.read(buffer))!=-1){if(job.cancel.get())throw new java.io.IOException("Download cancelled");count+=n;if(count>536870912)throw new java.io.IOException("Episode exceeds 512 MB download limit");out.write(buffer,0,n);job.status=total>0?"Downloading "+(100*count/total)+"%":"Downloading "+(count/1048576)+" MB";}out.getFD().sync();}
            if(job.cancel.get()||count==0||(total>=0&&count!=total))throw new java.io.IOException("Episode download did not complete");
            android.media.MediaMetadataRetriever verify=new android.media.MediaMetadataRetriever();try{verify.setDataSource(partial.toString());String duration=verify.extractMetadata(android.media.MediaMetadataRetriever.METADATA_KEY_DURATION);if(duration==null||Long.parseLong(duration)<=0)throw new java.io.IOException("Downloaded episode is not playable audio");}finally{verify.release();}
            java.io.File completed=new java.io.File(root,id+".audio");java.nio.file.Files.move(partial.toPath(),completed.toPath());
            addPodcast(context,new JSONObject().put("id",id).put("title",title).put("downloaded",System.currentTimeMillis()).put("pinned",false));job.status="Episode available offline";
        }catch(Exception error){job.error=error.getMessage();job.status="Episode download stopped; completed downloads retained";}finally{if(connection!=null)connection.disconnect();if(partial!=null)partial.delete();job.done=true;}});return new JSONObject().put("job",id);
    }
}
