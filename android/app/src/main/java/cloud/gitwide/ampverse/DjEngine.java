package cloud.gitwide.ampverse;

import android.content.*;
import android.media.*;
import android.os.*;
import androidx.media3.common.*;
import androidx.media3.exoplayer.ExoPlayer;
import androidx.media3.session.MediaSession;
import java.util.*;
import org.json.*;

/** Service-owned decks. Views never own or release live playback. */
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
final class DjEngine {
    static final int AMBER=0xffffbf24, CYAN=0xff22d4ed;
    final ExoPlayer[] decks=new ExoPlayer[4]; // A, B, left cue, right cue
    final MediaItem[] staged=new MediaItem[4];
    final long[] stagedPosition=new long[4];
    final boolean[] cueMode=new boolean[2],collapsed=new boolean[2];
    final float[] samplerVolume={.7f,.7f};
    final String[][] samples=new String[2][9], names=new String[2][9];
    final boolean[][] loops=new boolean[2][9];
    final int[][] sampleIds=new int[2][9], streams=new int[2][9];
    private final Set<Integer> ready=new HashSet<>();
    private final SoundPool pool;
    private final SharedPreferences prefs;
    private final Context context;
    private final List<MediaSession> sessions=new ArrayList<>();
    private final Handler handler=new Handler(Looper.getMainLooper());
    private final AudioManager manager;
    private final AudioFocusRequest focus;
    private final Runnable mixTick=this::advanceMix;
    private long mixStarted,mixWaitingSince;
    private float mixFrom,mixTo;
    private long mixDuration=8000;
    int fadeSeconds=3;
    private int mixOutgoing;
    private boolean mixing,closed;
    final List<MediaItem> autoQueue=new ArrayList<>();
    boolean auto;
    private final Set<String> requests=new LinkedHashSet<>();
    private int nextAuto;
    private JSONObject autoRules=new JSONObject();
    private final List<MediaItem> autoSource=new ArrayList<>();
    private final Deque<String> recentAuto=new ArrayDeque<>();
    private void rememberAuto(MediaItem item){if(item==null)return;recentAuto.addLast(item.mediaId);while(recentAuto.size()>Math.max(1,autoRules.optInt("recent_repeat",5)))recentAuto.removeFirst();}
    private boolean refillAuto(){if(!autoRules.optBoolean("smart")||autoRules.optString("fallback","relax_recent").equals("stop")||autoSource.isEmpty())return false;String current=decks[fader<.5?0:1].getCurrentMediaItem()==null?"":decks[fader<.5?0:1].getCurrentMediaItem().mediaId;List<MediaItem> next=DjLogic.repeatCandidates(autoSource,recentAuto,current,autoRules.optInt("recent_repeat",5));if(next.isEmpty())return false;autoQueue.clear();autoQueue.addAll(next);nextAuto=0;return true;}

    private final Runnable autoTick=this::advanceAuto;
    float fader=0,master=.75f;
    int destination=0;
    String message="Load a song into A or B. Cue decks are independent.";
    DjEngine(PlaybackService service) {
        context=service;prefs=service.getSharedPreferences("dj",Context.MODE_PRIVATE);fadeSeconds=Math.max(1,Math.min(30,prefs.getInt("fade-seconds",3)));
        manager=(AudioManager)service.getSystemService(Context.AUDIO_SERVICE);
        focus=new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_MEDIA).build()).setOnAudioFocusChangeListener(change->{if(change<0)pauseAll();},handler).build();
        for(int i=0;i<4;i++) {
            decks[i]=new ExoPlayer.Builder(service,PlaybackService.renderers(service)).build();
            decks[i].setAudioAttributes(new androidx.media3.common.AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(),false);
            decks[i].setHandleAudioBecomingNoisy(true);decks[i].setWakeMode(C.WAKE_MODE_LOCAL);
            final int deck=i;
            decks[i].addListener(new Player.Listener(){@Override public void onPlayWhenReadyChanged(boolean ready,int reason){if(ready)PlaybackService.claimDj();}@Override public void onPlayerError(PlaybackException error){message=title(deck)+": "+error.getErrorCodeName();}});
            MediaSession session=new MediaSession.Builder(service,decks[i]).setId("dj-"+i).build();sessions.add(session);service.addSession(session);
        }
        pool=new SoundPool.Builder().setMaxStreams(18).setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_MEDIA).setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC).build()).build();
        pool.setOnLoadCompleteListener((sound,id,status)->{if(status==0)ready.add(id);else message="Effect could not be decoded. Try a short WAV or MP3.";});
        for(int side=0;side<2;side++) {
            cueMode[side]=prefs.getBoolean("cue-"+side,false);collapsed[side]=prefs.getBoolean("collapsed-"+side,false);samplerVolume[side]=prefs.getFloat("sample-volume-"+side,.7f);
            decks[side+2].setVolume(prefs.getFloat("cue-volume-"+side,.7f));
            for(int pad=0;pad<9;pad++) {
                samples[side][pad]=prefs.getString("sample-"+side+"-"+pad,"");names[side][pad]=prefs.getString("name-"+side+"-"+pad,"PAD "+(pad+1));loops[side][pad]=prefs.getBoolean("loop-"+side+"-"+pad,false);
                if(!samples[side][pad].isEmpty()&&new java.io.File(samples[side][pad]).isFile())sampleIds[side][pad]=pool.load(samples[side][pad],1);
            }
        }
        destination=prefs.getInt("destination",0)==1?1:0;setFader(0);
        for(int i=0;i<4;i++){String uri=prefs.getString("prepared-uri-"+i,"");if(!uri.isEmpty())try{android.net.Uri parsed=android.net.Uri.parse(uri);java.io.File file=new java.io.File(parsed.getPath());if(("content".equals(parsed.getScheme())&&readable(service,parsed))||("file".equals(parsed.getScheme())&&file.isFile()&&file.getCanonicalPath().startsWith(service.getFilesDir().getCanonicalPath()+java.io.File.separator))){MediaItem item=new MediaItem.Builder().setMediaId(prefs.getString("prepared-id-"+i,file.getName())).setUri(parsed).setMediaMetadata(new androidx.media3.common.MediaMetadata.Builder().setTitle(prefs.getString("prepared-title-"+i,file.getName())).setExtras(preparedExtras(prefs.getString("prepared-metadata-"+i,"{}"))).build()).build();decks[i].setMediaItem(item,prefs.getLong("prepared-position-"+i,0));decks[i].prepare();decks[i].pause();}}catch(Exception error){message="A previously prepared phone track is unavailable.";}}

    }
    private static boolean readable(Context context,android.net.Uri uri){try(android.content.res.AssetFileDescriptor descriptor=context.getContentResolver().openAssetFileDescriptor(uri,"r")){return descriptor!=null;}catch(Exception e){return false;}}
    static JSONObject preparation(MediaItem item){try{JSONObject prep=new JSONObject(item!=null&&item.mediaMetadata.extras!=null?item.mediaMetadata.extras.getString("amp-preparation","{}"):"{}");if(prep.optBoolean("sync_source_changed")&&!prep.optBoolean("manual_override")){for(String key:new String[]{"bpm","beat_offset","key","harmonic_key","analysis","energy_profile","energy","stems","cues"})prep.remove(key);prep.put("cue_in",0).remove("cue_out");}return prep;}catch(Exception e){return new JSONObject();}}
    static Bundle preparedExtras(String json){Bundle extras=new Bundle();extras.putString("amp-preparation",json);return extras;}
    static long cueIn(MediaItem item){return DjLogic.cueIn(item);}
    static long cueOut(MediaItem item,long duration){return DjLogic.cueOut(item,duration);}
    void fadeSeconds(int value){fadeSeconds=Math.max(1,Math.min(30,value));prefs.edit().putInt("fade-seconds",fadeSeconds).apply();}
    void request(MediaItem item,int within){
        if(within<1||within>5){message="Choose 1–5 songs.";return;}
        if(autoQueue.isEmpty()){message="Load an Auto DJ playlist first.";return;}
        if(requests.contains(item.mediaId)){message="That song is already requested.";return;}
        int slot=requests.size()+1+(mixing?1:0);
        if(slot>within){message="That request window is full; wait for a song to play.";return;}
        for(int i=nextAuto;i<autoQueue.size();i++)if(autoQueue.get(i).mediaId.equals(item.mediaId)){autoQueue.remove(i);break;}
        autoQueue.add(Math.min(autoQueue.size(),nextAuto+requests.size()),item);requests.add(item.mediaId);
        message="Requested "+item.mediaMetadata.title+" within "+slot+" songs.";
    }

    String title(int deck){MediaItem item=decks[deck].getCurrentMediaItem();return item==null?"No track loaded":String.valueOf(item.mediaMetadata.title);}
    boolean active(int deck){return decks[deck].getPlayWhenReady()&&decks[deck].getPlaybackState()!=Player.STATE_ENDED;}
    void load(int deck,MediaItem item,long position) {
        if(active(deck)||(deck<2&&mixing)) {staged[deck]=item;stagedPosition[deck]=position;message="Replacement staged. Stop this deck, then Apply staged track.";return;}
        decks[deck].setPlaybackParameters(PlaybackParameters.DEFAULT);
        decks[deck].setMediaItem(item,Math.max(cueIn(item),position));decks[deck].prepare();decks[deck].pause();staged[deck]=null;checkpoint();message="Track prepared; press Play when ready.";
    }
    void apply(int deck){if(staged[deck]!=null&&!active(deck)){MediaItem item=staged[deck];long position=stagedPosition[deck];load(deck,item,position);}}
    void play(int deck){if(decks[deck].getCurrentMediaItem()==null){message="Load a song first.";return;}if(deck<2&&manager.requestAudioFocus(focus)!=AudioManager.AUDIOFOCUS_REQUEST_GRANTED){message="Audio focus unavailable.";return;}if(decks[deck].getPlaybackState()==Player.STATE_IDLE)decks[deck].prepare();if(decks[deck].getPlaybackState()==Player.STATE_ENDED)decks[deck].seekTo(cueIn(decks[deck].getCurrentMediaItem()));decks[deck].play();}
    void stop(int deck){if(deck<2)cancelMix();decks[deck].pause();decks[deck].seekTo(cueIn(decks[deck].getCurrentMediaItem()));}
    void seek(int deck,long value){decks[deck].seekTo(Math.max(0,value));checkpoint();}
    void setFader(float value){fader=Math.max(0,Math.min(1,value));decks[0].setVolume((float)Math.cos(fader*Math.PI/2)*master);decks[1].setVolume((float)Math.sin(fader*Math.PI/2)*master);}
    void cancelMix(){mixing=false;handler.removeCallbacks(mixTick);}
    boolean mixing(){return mixing;}
    void mixNext(){
        if(mixing){cancelMix();message="Automatic crossfade cancelled.";return;}
        mixOutgoing=fader<.5?0:1;int next=1-mixOutgoing;
        if(auto){
            if(nextAuto>=autoQueue.size()&&!refillAuto()){message="No next song available in this playlist.";return;}
            MediaItem queued=autoQueue.get(nextAuto),loaded=decks[next].getCurrentMediaItem();
            if(!active(next)&&(loaded==null||!queued.mediaId.equals(loaded.mediaId)))load(next,queued,DjLogic.entry(queued,autoRules));
        }
        if(decks[next].getCurrentMediaItem()==null){message="Load the other deck before mixing.";return;}
        if(auto)decks[next].setPlaybackParameters(new PlaybackParameters(DjLogic.tempoRatio(decks[mixOutgoing].getCurrentMediaItem(),decks[next].getCurrentMediaItem(),decks[mixOutgoing].getPlaybackParameters().speed),1));
        play(next);if(!active(next))return;mixFrom=fader;mixTo=next;mixStarted=0;mixWaitingSince=SystemClock.uptimeMillis();mixing=true;
        if(auto&&nextAuto<autoQueue.size()&&autoQueue.get(nextAuto).mediaId.equals(decks[next].getCurrentMediaItem().mediaId)){requests.remove(autoQueue.get(nextAuto).mediaId);rememberAuto(autoQueue.get(nextAuto));nextAuto++;}
        handler.post(mixTick);
    }
    private void advanceMix(){
        if(!mixing||closed)return;
        int incoming=1-mixOutgoing;
        if(!decks[incoming].isPlaying()){if(decks[incoming].getPlayerError()!=null||decks[incoming].getPlaybackState()==Player.STATE_ENDED||SystemClock.uptimeMillis()-mixWaitingSince>15000){cancelMix();decks[incoming].pause();message="Could not prepare the next song; current deck retained.";}else handler.postDelayed(mixTick,30);return;}
        if(mixStarted==0){mixStarted=SystemClock.uptimeMillis();long incomingDuration=decks[incoming].getDuration(),outgoingRemaining=decks[mixOutgoing].getDuration()-decks[mixOutgoing].getCurrentPosition();mixDuration=(long)(SharedDjLogic.fade(fadeSeconds,outgoingRemaining>0?outgoingRemaining/1000.:Double.NaN)*1000);if(incomingDuration>0)mixDuration=Math.min(mixDuration,Math.max(250,incomingDuration-decks[incoming].getCurrentPosition()-100));if(outgoingRemaining>0)mixDuration=Math.min(mixDuration,Math.max(250,outgoingRemaining));}float progress=Math.min(1,(SystemClock.uptimeMillis()-mixStarted)/(float)mixDuration);
        setFader(mixFrom+(mixTo-mixFrom)*progress);
        if(progress>=1){decks[mixOutgoing].pause();mixing=false;message="Mix complete.";}else handler.postDelayed(mixTick,30);
    }
    void queue(List<MediaItem> items,JSONObject rules){queue(items);try{autoRules=rules==null?new JSONObject():new JSONObject(rules.toString());}catch(JSONException e){autoRules=new JSONObject();}}
    void queue(List<MediaItem> items){autoRules=new JSONObject();autoSource.clear();autoSource.addAll(items);recentAuto.clear();auto=false;cancelMix();handler.removeCallbacks(autoTick);autoQueue.clear();requests.clear();autoQueue.addAll(items);nextAuto=0;message="Loaded "+items.size()+" songs for Auto DJ. Press Start Auto DJ.";}
    void auto(boolean enabled){
        if(!enabled){auto=false;cancelMix();handler.removeCallbacks(autoTick);message="Auto DJ off; current playback retained.";return;}
        if(autoQueue.isEmpty()){message="Choose a playlist for Auto DJ first.";return;}
        if(active(0)&&active(1)){message="Stop one main deck before starting Auto DJ.";return;}
        decks[0].setRepeatMode(Player.REPEAT_MODE_OFF);decks[1].setRepeatMode(Player.REPEAT_MODE_OFF);
        int current=active(1)?1:0;setFader(current);
        if(!active(current)){load(current,autoQueue.get(0),DjLogic.entry(autoQueue.get(0),autoRules));play(current);rememberAuto(autoQueue.get(0));nextAuto=1;}else{nextAuto=0;MediaItem loaded=decks[current].getCurrentMediaItem();for(int i=0;i<autoQueue.size();i++)if(loaded!=null&&loaded.mediaId.equals(autoQueue.get(i).mediaId)){nextAuto=i+1;break;}}
        if(!active(current)){message="Could not start playback; Auto DJ remains off.";return;}
        auto=true;handler.post(autoTick);message="Auto DJ running.";
    }
    private void advanceAuto(){
        if(!auto||closed)return;
        if(!mixing){int current=fader<.5?0:1,other=1-current;
            if(nextAuto>=autoQueue.size()&&!refillAuto()){if(!active(current)){auto=false;message="Playlist finished.";}else handler.postDelayed(autoTick,200);return;}
            if(!active(other)){MediaItem next=autoQueue.get(nextAuto);MediaItem loaded=decks[other].getCurrentMediaItem();if(loaded==null||!next.mediaId.equals(loaded.mediaId)){load(other,next,DjLogic.entry(next,autoRules));}}
            long duration=cueOut(decks[current].getCurrentMediaItem(),decks[current].getDuration());if(duration>0&&duration-decks[current].getCurrentPosition()<=fadeSeconds*1000L){mixNext();}
            else if(decks[current].getPlaybackState()==Player.STATE_ENDED){mixNext();}
        }
        if(auto)handler.postDelayed(autoTick,200);
    }
    void mode(int side,boolean cue){cueMode[side]=cue;prefs.edit().putBoolean("cue-"+side,cue).apply();}
    void collapse(int side){collapsed[side]=!collapsed[side];prefs.edit().putBoolean("collapsed-"+side,collapsed[side]).apply();}
    void destination(int deck){destination=deck;prefs.edit().putInt("destination",deck).apply();}
    void cueVolume(int side,float volume){decks[side+2].setVolume(volume);prefs.edit().putFloat("cue-volume-"+side,volume).apply();}
    void sampleVolume(int side,float volume){samplerVolume[side]=volume;prefs.edit().putFloat("sample-volume-"+side,volume).apply();for(int stream:streams[side])if(stream!=0)pool.setVolume(stream,volume,volume);}
    void assign(int side,int pad,String path,String name){
        stopPad(side,pad);int old=sampleIds[side][pad];if(old!=0){pool.unload(old);ready.remove(old);}
        samples[side][pad]=path;names[side][pad]=name;sampleIds[side][pad]=pool.load(path,1);
        prefs.edit().putString("sample-"+side+"-"+pad,path).putString("name-"+side+"-"+pad,name).apply();message="Effect assigned. Tap its pad to play.";
    }
    void trigger(int side,int pad){int id=sampleIds[side][pad];if(!ready.contains(id)){message="Load a short effect first, or wait for decoding.";return;}PlaybackService.claimDj();stopPad(side,pad);streams[side][pad]=pool.play(id,samplerVolume[side],samplerVolume[side],1,loops[side][pad]?-1:0,1);}
    boolean playerRepeats(){return autoRules.optBoolean("smart")&&!autoRules.optString("fallback","relax_recent").equals("stop");}
    long playerPosition(){return decks[fader<.5?0:1].getCurrentPosition();}
    List<MediaItem> playerQueue(){
        List<MediaItem> result=new ArrayList<>();MediaItem current=decks[fader<.5?0:1].getCurrentMediaItem();
        if(current!=null)result.add(current);
        for(int i=nextAuto;i<autoQueue.size();i++){MediaItem item=autoQueue.get(i);if(i!=nextAuto||current==null||!item.mediaId.equals(current.mediaId))result.add(item);}
        if(playerRepeats())for(MediaItem item:autoSource){boolean included=false;for(MediaItem existing:result)if(existing.mediaId.equals(item.mediaId)){included=true;break;}if(!included)result.add(item);}
        return result;
    }
    void pauseForPlayer(){auto(false);for(ExoPlayer deck:decks)deck.pause();for(int side=0;side<2;side++)for(int pad=0;pad<9;pad++)stopPad(side,pad);}
    void stopPad(int side,int pad){if(streams[side][pad]!=0)pool.stop(streams[side][pad]);streams[side][pad]=0;}
    void clearPad(int side,int pad){stopPad(side,pad);pool.unload(sampleIds[side][pad]);ready.remove(sampleIds[side][pad]);sampleIds[side][pad]=0;samples[side][pad]="";names[side][pad]="PAD "+(pad+1);prefs.edit().remove("sample-"+side+"-"+pad).remove("name-"+side+"-"+pad).apply();}
    void loop(int side,int pad,boolean value){loops[side][pad]=value;prefs.edit().putBoolean("loop-"+side+"-"+pad,value).apply();if(streams[side][pad]!=0)pool.setLoop(streams[side][pad],value?-1:0);}
    void pauseAll(){auto=false;handler.removeCallbacks(autoTick);cancelMix();for(ExoPlayer deck:decks)deck.pause();for(int s=0;s<2;s++)for(int p=0;p<9;p++)stopPad(s,p);}
    void checkpoint(){SharedPreferences.Editor edit=prefs.edit();for(int i=0;i<4;i++){MediaItem item=decks[i].getCurrentMediaItem();if(item!=null&&item.localConfiguration!=null)edit.putString("prepared-uri-"+i,item.localConfiguration.uri.toString()).putString("prepared-title-"+i,String.valueOf(item.mediaMetadata.title)).putString("prepared-id-"+i,item.mediaId).putLong("prepared-position-"+i,decks[i].getCurrentPosition()).putString("prepared-metadata-"+i,preparation(item).toString());else edit.remove("prepared-uri-"+i).remove("prepared-title-"+i).remove("prepared-id-"+i).remove("prepared-position-"+i);}edit.apply();}
    void close(PlaybackService service){closed=true;pauseAll();checkpoint();manager.abandonAudioFocusRequest(focus);for(MediaSession session:sessions){service.removeSession(session);session.release();}for(ExoPlayer deck:decks)deck.release();pool.release();}
}
