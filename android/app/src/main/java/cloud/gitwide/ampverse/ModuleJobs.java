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
    static synchronized JSONObject privateDownload(Context context,JSONObject args)throws Exception {
        if(jobs.values().stream().anyMatch(job->!job.done))throw new IllegalStateException("A download is already running");
        String id=UUID.randomUUID().toString();Job job=new Job();jobs.put(id,job);JSONObject captured=new JSONObject(args.toString());
        worker.execute(()->{java.io.File stage=null;boolean published=false;try{
            String password=captured.getString("password");if(password.isEmpty())throw new IllegalArgumentException("Private library access required");
            CastClient client=new CastClient(captured.getString("base"),password);JSONObject status=client.connect();String server=captured.getString("server_id"),library=captured.getString("library_id"),path=CastLibraryModel.path(captured.getString("path"));
            if(!server.equals(status.optString("server_id"))||!library.equals(status.optString("library_id")))throw new IllegalStateException("Selected server/library changed");
            JSONObject caps=status.optJSONObject("capabilities");if(caps==null||!"library_path_v1".equals(caps.optString("private_audio")))throw new IllegalStateException("Update selected server for private audio download");
            String target=android.net.Uri.parse(client.base).buildUpon().path("/api/player/audio").appendQueryParameter("library_id",library).appendQueryParameter("path",path).appendQueryParameter("download","true").build().toString();
            java.io.File root=new java.io.File(context.getFilesDir(),"radio-offline");if(!root.isDirectory()&&!root.mkdirs())throw new java.io.IOException("Cannot create private offline folder");stage=new java.io.File(root,".pending-"+id);if(!stage.mkdir())throw new java.io.IOException("Cannot stage private download");
            String extension=path.substring(path.lastIndexOf('.')+1);if(!extension.matches("[A-Za-z0-9]{1,8}"))extension="audio";String filename="track."+extension;java.io.File audio=new java.io.File(stage,filename);PrivateFileTransfer.Result transfer;
            try(okhttp3.Response response=PrivateLibraryHttp.create(client.base,password,server,library,target).newCall(new okhttp3.Request.Builder().url(target).build()).execute()){
                if(response.code()!=200||response.body()==null)throw new java.io.IOException("Private download rejected");long expected=response.body().contentLength();if(expected>536870912)throw new java.io.IOException("Song exceeds 512 MB download limit");
                try(java.io.InputStream in=response.body().byteStream()){transfer=PrivateFileTransfer.copy(in,audio,expected,536870912,job.cancel::get,bytes->job.status="Downloading "+(bytes/1048576)+" MB");}
            }
            android.media.MediaMetadataRetriever verify=new android.media.MediaMetadataRetriever();try{verify.setDataSource(audio.toString());String duration=verify.extractMetadata(android.media.MediaMetadataRetriever.METADATA_KEY_DURATION);if(duration==null||Long.parseLong(duration)<=0)throw new java.io.IOException("Downloaded file is not playable audio");}finally{verify.release();}
            if(job.cancel.get())throw new java.io.IOException("Download cancelled");
            String title=captured.optString("title",path.substring(path.lastIndexOf('/')+1));
            JSONObject source=new JSONObject().put("server_id",server).put("library_id",library).put("station_id",status.getString("station_id"));
            JSONObject item=new JSONObject().put("file",filename).put("title",title).put("media_id","station-"+id+"-"+filename).put("track_key",PlaylistSyncModel.trackKey(path)).put("bytes",transfer.bytes).put("preparation",new JSONObject().put("source_sha256",transfer.sha256));
            JSONObject album=new JSONObject().put("version",2).put("name",title).put("source",source).put("playlist",new JSONObject().put("id","private-"+id).put("name",title).put("tracks",new JSONArray())).put("items",new JSONArray().put(item)).put("conflicts",new JSONArray()).put("offline_changes","local_only");
            CatalogStore.put(new java.io.File(stage,"ampdj/catalog.ampdb"),"offline-playlist",album.toString());if(job.cancel.get())throw new java.io.IOException("Download cancelled");
            java.nio.file.Files.move(stage.toPath(),new java.io.File(root,id).toPath(),java.nio.file.StandardCopyOption.ATOMIC_MOVE);published=true;job.status="Available in Offline playlists · audio only; prepared cues were not downloaded";
        }catch(Exception error){job.error="Private song download failed; previous offline files and playback were retained";job.status="Download stopped";}finally{if(!published&&stage!=null)removePrivateStage(stage);job.done=true;}});
        return new JSONObject().put("job",id);
    }
    private static void removePrivateStage(java.io.File path){java.io.File[] files=path.listFiles();if(files!=null)for(java.io.File child:files)removePrivateStage(child);path.delete();}
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
