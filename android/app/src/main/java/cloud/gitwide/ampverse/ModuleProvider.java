package cloud.gitwide.ampverse;

import android.content.*;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.net.Uri;
import android.os.*;
import androidx.media3.common.*;
import org.json.*;
import java.util.*;
import java.util.concurrent.*;

/** Versioned, same-signer IPC. Companions never read another app's private paths. */
public final class ModuleProvider extends ContentProvider {
    public boolean onCreate(){return true;}
    @Override public Bundle call(String method,String argument,Bundle extras){
        if(getContext().getPackageManager().checkSignatures(Binder.getCallingUid(),android.os.Process.myUid())!=PackageManager.SIGNATURE_MATCH)throw new SecurityException("AMP module signer mismatch");
        if(!"v1".equals(method))throw new IllegalArgumentException("Unsupported AMP module interface");
        String body=extras==null?"{}":extras.getString("json","{}");
        if(body.length()>256*1024)throw new IllegalArgumentException("Module command is too large");
        final JSONObject request;final Selection selection;
        try{request=new JSONObject(body);selection=prepare(argument,request);}catch(Exception error){Bundle result=new Bundle();result.putString("error",error.getMessage());return result;}
        FutureTask<Bundle> task=new FutureTask<>(()->{
            Bundle result=new Bundle();
            try{result.putString("json",dispatch(argument,request,selection).toString());}
            catch(Exception error){if(selection!=null&&selection.sample()!=null&&!SamplerStore.referenced(getContext().getSharedPreferences("dj",0),selection.sample().file()))selection.sample().file().delete();result.putString("error",error.getMessage()==null?error.toString():error.getMessage());}
            return result;
        });
        // Catalog reads, settings and queue construction never occupy the audio/UI thread.
        if(Arrays.asList("hello","library","playlists","module_read","module_write","sync_cast","podcast_download","podcast_downloads","job","offline_lists","sampler_slots").contains(argument)){task.run();try{return task.get();}catch(Exception error){throw new IllegalStateException(error);}}
        new Handler(Looper.getMainLooper()).post(task);
        try{return task.get(5,TimeUnit.SECONDS);}catch(Exception error){task.cancel(false);throw new IllegalStateException("AMP Player is busy; retry the command",error);}
    }
    private record Selection(List<MediaItem> items,JSONObject rules,SamplerStore.Imported sample){Selection(List<MediaItem> items,JSONObject rules){this(items,rules,null);}}
    private Selection prepare(String action,JSONObject args)throws Exception{
        Context context=getContext();
        if("sampler_import".equals(action))return new Selection(Collections.emptyList(),null,SamplerStore.prepare(context,args));
        if("load".equals(action)){for(MediaItem item:LibraryStore.read(context))if(item.mediaId.equals(args.getString("id")))return new Selection(Collections.singletonList(item),null);throw new IllegalArgumentException("Song is no longer in the library");}
        if("queue".equals(action)){JSONArray lists=PhonePlaylists.read(context);for(int i=0;i<lists.length();i++){JSONObject list=lists.getJSONObject(i);if(list.getString("id").equals(args.getString("id"))){List<MediaItem> items=PhonePlaylists.tracks(context,list);if(items.isEmpty())throw new IllegalArgumentException("This playlist has no matching local songs");return new Selection(items,list);}}throw new IllegalArgumentException("Playlist unavailable");}
        if("offline_play".equals(action)||"offline_queue".equals(action)){for(java.io.File folder:RadioOffline.albums(context))if(folder.getName().equals(args.getString("id"))){List<MediaItem> items=RadioOffline.items(folder);if(items.isEmpty())throw new IllegalStateException("Offline playlist is empty");return new Selection(items,null);}throw new IllegalArgumentException("Offline playlist not found");}
        return null;
    }
    private JSONObject dispatch(String action,JSONObject args,Selection selection)throws Exception {
        Context context=getContext();
        if("sampler_slots".equals(action))return SamplerStore.slots(context.getSharedPreferences("dj",0));
        if("sampler_import".equals(action)){SamplerStore.Imported sample=selection.sample();SharedPreferences prefs=context.getSharedPreferences("dj",0);SamplerStore.expect(prefs,sample.side(),sample.pad(),sample.expected());DjEngine engine=PlaybackService.existingDj();if(engine!=null)engine.assign(sample.side(),sample.pad(),sample.file().toString(),sample.name(),sample.loop());else SamplerStore.save(prefs,sample.side(),sample.pad(),sample.file().toString(),sample.name(),sample.loop());return new JSONObject().put("message","Clip saved to sampler "+(sample.side()==0?"A":"B")+" pad "+(sample.pad()+1));}
        if("sampler_clear".equals(action)){int side=args.getInt("side"),pad=args.getInt("pad");SharedPreferences prefs=context.getSharedPreferences("dj",0);SamplerStore.expect(prefs,side,pad,args.getLong("revision"));DjEngine engine=PlaybackService.existingDj();if(engine!=null)engine.clearPad(side,pad);else SamplerStore.clear(prefs,side,pad);return new JSONObject().put("message","Sampler pad cleared; source files retained");}
        if("hello".equals(action))return new JSONObject().put("api",1).put("core",BuildConfig.VERSION_CODE).put("edition",BuildConfig.EDITION);
        if("library".equals(action)){
            List<MediaItem> all=LibraryStore.read(context);JSONArray rows=new JSONArray();String query=args.optString("query").toLowerCase(Locale.ROOT);int offset=Math.max(0,args.optInt("offset")),matched=0;
            for(MediaItem item:all){String title=String.valueOf(item.mediaMetadata.title),artist=String.valueOf(item.mediaMetadata.artist);if(!(title+" "+artist).toLowerCase(Locale.ROOT).contains(query))continue;if(matched++<offset)continue;if(rows.length()<50)rows.put(new JSONObject().put("id",item.mediaId).put("title",title).put("artist",artist));}
            return new JSONObject().put("items",rows).put("total",matched);
        }
        if("playlists".equals(action)){JSONArray stored=PhonePlaylists.read(context),summary=new JSONArray();for(int i=0;i<stored.length();i++){JSONObject list=stored.getJSONObject(i);summary.put(new JSONObject().put("id",list.optString("id")).put("name",list.optString("name")).put("smart",list.optBoolean("smart")));}return new JSONObject().put("items",summary);}
        if("sync_cast".equals(action))return ModuleJobs.syncCast(context,args);
        if("podcast_download".equals(action))return ModuleJobs.podcast(context,args);
        if("podcast_downloads".equals(action))return new JSONObject().put("items",ModuleJobs.podcastDownloads(context));
        if("job".equals(action))return ModuleJobs.state(args.getString("job"),args.optBoolean("cancel"));
        if("offline_lists".equals(action)){JSONArray lists=new JSONArray();for(java.io.File folder:RadioOffline.albums(context))lists.put(new JSONObject().put("id",folder.getName()).put("name",RadioOffline.album(folder).optString("name")));return new JSONObject().put("items",lists);}
        if("module_read".equals(action)||"module_write".equals(action)){
            String key=args.getString("key");if(!key.matches("(podcasts|cast|remote|lab|dj)\\.[a-z0-9_-]{1,40}"))throw new IllegalArgumentException("Invalid module settings key");
            SharedPreferences prefs=context.getSharedPreferences("modules",0);
            if(action.equals("module_write")&&!prefs.edit().putString(key,args.getString("value")).commit())throw new IllegalStateException("Could not save module settings");
            return new JSONObject().put("value",prefs.getString(key,"{}"));
        }
        if("player_pause".equals(action)){PlaybackService.pauseForLab();return new JSONObject();}
        Player player=PlaybackService.player();
        if(player==null)throw new IllegalStateException("Open AMP Player to connect its audio service.");
        if("podcast_offline".equals(action)){
            JSONArray downloads=ModuleJobs.podcastDownloads(context);for(int i=0;i<downloads.length();i++){JSONObject item=downloads.getJSONObject(i);String id=item.getString("id");if(id.equals(args.getString("id"))&&id.matches("[a-f0-9-]{36}")){java.io.File file=new java.io.File(context.getFilesDir(),"podcasts/downloads/"+id+".audio");if(!file.isFile())throw new IllegalStateException("Episode is no longer downloaded");player.setMediaItem(new MediaItem.Builder().setMediaId("amp-podcast:"+id).setUri(Uri.fromFile(file)).setMediaMetadata(new MediaMetadata.Builder().setTitle(item.optString("title")).build()).build());player.prepare();player.play();return new JSONObject();}}throw new IllegalArgumentException("Episode is not downloaded");
        }
        if("offline_play".equals(action)||"offline_queue".equals(action)){
            if(action.equals("offline_queue"))PlaybackService.dj().queue(selection.items());else{player.setMediaItems(selection.items());player.prepare();player.play();}return new JSONObject().put("message","Loaded offline playlist with prepared cues");
        }
        if("player_stream".equals(action)){
            String url=args.getString("url");Uri uri=Uri.parse(url);if(!Arrays.asList("https","http","content").contains(uri.getScheme()))throw new IllegalArgumentException("Unsupported media source");
            player.setMediaItem(new MediaItem.Builder().setMediaId(args.optString("kind","amp-radio")).setUri(uri).setMediaMetadata(new MediaMetadata.Builder().setTitle(args.optString("title","AMP audio")).build()).build());player.prepare();player.play();return new JSONObject();
        }
        DjEngine dj=PlaybackService.dj();int deck=args.optInt("deck",0);if(deck<0||deck>3)throw new IllegalArgumentException("Choose deck A, B or a Cue Deck");
        switch(action){
            case "state":break;
            case "load":{MediaItem selected=selection.items().get(0);dj.load(deck,selected,DjLogic.cueIn(selected));break;}
            case "play":dj.play(deck);break;
            case "pause":dj.decks[deck].pause();break;
            case "cue":dj.stop(deck);break;
            case "seek":dj.seek(deck,args.getLong("position"));break;
            case "fader":{double value=args.getDouble("value");if(!Double.isFinite(value)||value<0||value>1)throw new IllegalArgumentException("Crossfader must be between A and B");dj.setFader((float)value);break;}
            case "auto":dj.auto(args.getBoolean("enabled"));break;
            case "mix":dj.mixNext();break;
            case "fade":dj.fadeSeconds(args.getInt("seconds"));break;
            case "queue":dj.queue(selection.items(),selection.rules());break;
            case "hotcue":{int slot=args.getInt("slot");if(slot<0||slot>7)throw new IllegalArgumentException("Invalid cue");MediaItem item=dj.decks[deck].getCurrentMediaItem();if(item==null)throw new IllegalStateException("Load a song first");long at=PhoneCues.seconds(context,item,slot);if(at<0||args.optBoolean("save")){PhoneCues.save(context,item,slot,dj.decks[deck].getCurrentPosition());}else dj.seek(deck,at);break;}
            case "sample":{int side=args.optInt("side"),pad=args.getInt("pad");if(side<0||side>1||pad<0||pad>8)throw new IllegalArgumentException("Invalid sampler pad");dj.trigger(side,pad);break;}
            default:throw new IllegalArgumentException("Unsupported AMP module command");
        }
        JSONArray decks=new JSONArray();for(int i=0;i<4;i++)decks.put(new JSONObject().put("title",dj.title(i)).put("position",dj.decks[i].getCurrentPosition()).put("duration",Math.max(0,dj.decks[i].getDuration())).put("playing",dj.active(i)));
        return new JSONObject().put("owner",PlaybackService.owner()).put("auto",dj.auto).put("fader",dj.fader).put("fade",dj.fadeSeconds).put("message",dj.message).put("decks",decks);
    }
    public Cursor query(Uri u,String[] p,String s,String[] a,String order){throw new UnsupportedOperationException();}
    public String getType(Uri u){return null;}
    public Uri insert(Uri u,ContentValues v){throw new UnsupportedOperationException();}
    public int update(Uri u,ContentValues v,String s,String[] a){throw new UnsupportedOperationException();}
    public int delete(Uri u,String s,String[] a){throw new UnsupportedOperationException();}
}
