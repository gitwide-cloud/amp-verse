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
    private final OwnedSamplerAudio ownedSamples;
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
    private VinylScratch scratch;
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
        focus=new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_MEDIA).build()).setOnAudioFocusChangeListener(change->{if(change<0){PlaybackService.suppressOwnedOutput(true);pauseAll();}},handler).build();
        for(int i=0;i<4;i++) {
            decks[i]=new ExoPlayer.Builder(service,PlaybackService.renderers(service,false,i==0?OwnedPcmMixer.Source.DECK_A:i==1?OwnedPcmMixer.Source.DECK_B:OwnedPcmMixer.Source.PRIVATE_CUE)).build();
            decks[i].setAudioAttributes(new androidx.media3.common.AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(),false);
            decks[i].setHandleAudioBecomingNoisy(true);decks[i].setWakeMode(C.WAKE_MODE_LOCAL);
            final int deck=i;
            decks[i].addListener(new Player.Listener(){@Override public void onPlayWhenReadyChanged(boolean ready,int reason){if(ready){if(scratch!=null&&scratch.busy(deck))scratch.invalidate(deck);if(deck<2)PlaybackService.claimDj();}}@Override public void onPlayerError(PlaybackException error){if(scratch!=null)scratch.invalidate(deck);message=title(deck)+": "+error.getErrorCodeName();}@Override public void onMediaItemTransition(MediaItem item,int reason){if(deck<2)PlaybackService.phoneSourceChanged();if(scratch!=null)scratch.invalidate(deck);}@Override public void onPositionDiscontinuity(Player.PositionInfo oldPosition,Player.PositionInfo newPosition,int reason){if(scratch!=null&&scratch.busy(deck))scratch.invalidate(deck);}});
            // Remote/notification commands silence scratch before the normal player can run.
            Player guarded=new ForwardingPlayer(decks[i]){
                private void stopScratch(){if(scratch!=null)scratch.invalidate(deck);}
                @Override public void play(){stopScratch();super.play();}
                @Override public void pause(){stopScratch();super.pause();}
                @Override public void setPlayWhenReady(boolean ready){stopScratch();super.setPlayWhenReady(ready);}
                @Override public void seekTo(long position){stopScratch();super.seekTo(position);}
                @Override public void seekTo(int index,long position){stopScratch();super.seekTo(index,position);}
                @Override public void stop(){stopScratch();super.stop();}
            };
            MediaSession session=new MediaSession.Builder(service,guarded).setId("dj-"+i).build();sessions.add(session);service.addSession(session);
        }
        scratch=new VinylScratch(service,decks,text->message=text,PlaybackService::claimDjForScratch);
        OwnedAudioOutput owned=PlaybackService.ownedOutput();
        ownedSamples=owned==null?null:new OwnedSamplerAudio(owned,(id,success)->{if(success)ready.add(id);else message="Effect could not decode within the ten-second memory budget. Try a short local WAV or MP3.";});
        pool=ownedSamples!=null?null:new SoundPool.Builder().setMaxStreams(18).setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_MEDIA).setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC).build()).build();
        if(pool!=null)pool.setOnLoadCompleteListener((sound,id,status)->{if(status==0)ready.add(id);else message="Effect could not be decoded. Try a short WAV or MP3.";});
        for(int side=0;side<2;side++) {
            cueMode[side]=prefs.getBoolean("cue-"+side,false);collapsed[side]=prefs.getBoolean("collapsed-"+side,false);samplerVolume[side]=prefs.getFloat("sample-volume-"+side,.7f);
            decks[side+2].setVolume(prefs.getFloat("cue-volume-"+side,.7f));
            for(int pad=0;pad<9;pad++) {
                samples[side][pad]=prefs.getString("sample-"+side+"-"+pad,"");names[side][pad]=prefs.getString("name-"+side+"-"+pad,"PAD "+(pad+1));loops[side][pad]=prefs.getBoolean("loop-"+side+"-"+pad,false);
                if(!samples[side][pad].isEmpty()&&new java.io.File(samples[side][pad]).isFile())sampleIds[side][pad]=loadSample(samples[side][pad]);
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
    boolean active(int deck){return scratch!=null&&scratch.busy(deck)||decks[deck].getPlayWhenReady()&&decks[deck].getPlaybackState()!=Player.STATE_ENDED;}
    void load(int deck,MediaItem item,long position) {
        if(active(deck)||(deck<2&&mixing)) {staged[deck]=item;stagedPosition[deck]=position;message="Replacement staged. Stop this deck, then Apply staged track.";return;}
        if(scratch!=null)scratch.invalidate(deck);
        decks[deck].setPlaybackParameters(PlaybackParameters.DEFAULT);
        decks[deck].setMediaItem(item,Math.max(cueIn(item),position));decks[deck].prepare();decks[deck].pause();staged[deck]=null;checkpoint();message="Track prepared; press Play when ready.";
    }
    void apply(int deck){if(staged[deck]!=null&&!active(deck)){MediaItem item=staged[deck];long position=stagedPosition[deck];load(deck,item,position);}}
    void play(int deck){if(scratch!=null&&scratch.busy(deck)){message="Release the vinyl before changing transport.";return;}if(decks[deck].getCurrentMediaItem()==null){message="Load a song first.";return;}if(deck<2&&manager.requestAudioFocus(focus)!=AudioManager.AUDIOFOCUS_REQUEST_GRANTED){message="Audio focus unavailable.";return;}if(decks[deck].getPlaybackState()==Player.STATE_IDLE)decks[deck].prepare();if(decks[deck].getPlaybackState()==Player.STATE_ENDED)decks[deck].seekTo(cueIn(decks[deck].getCurrentMediaItem()));decks[deck].play();}
    void pause(int deck){if(scratch!=null)scratch.end(deck,false);decks[deck].pause();}
    void stop(int deck){if(scratch!=null)scratch.invalidate(deck);if(deck<2)cancelMix();decks[deck].pause();decks[deck].seekTo(cueIn(decks[deck].getCurrentMediaItem()));}
    void seek(int deck,long value){if(scratch!=null)scratch.invalidate(deck);decks[deck].seekTo(Math.max(0,value));checkpoint();}
    void setFader(float value){if(ownedSamples!=null)ownedSamples.master(master);fader=Math.max(0,Math.min(1,value));decks[0].setVolume((float)Math.cos(fader*Math.PI/2)*master);decks[1].setVolume((float)Math.sin(fader*Math.PI/2)*master);if(scratch!=null){scratch.gain(0,decks[0].getVolume());scratch.gain(1,decks[1].getVolume());}}
    void cancelMix(){mixing=false;handler.removeCallbacks(mixTick);}
    boolean mixing(){return mixing;}
    void mixNext(){
        if(scratch!=null&&scratch.any()){message="Release the vinyl before starting a mix.";return;}
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
        if(enabled&&scratch!=null&&scratch.any()){message="Release the vinyl before enabling Auto Mix.";return;}
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
        if(scratch!=null&&scratch.any()){handler.postDelayed(autoTick,200);return;}
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
    void cueVolume(int side,float volume){decks[side+2].setVolume(volume);if(scratch!=null)scratch.gain(side+2,volume);prefs.edit().putFloat("cue-volume-"+side,volume).apply();}
    boolean mixerSettingsBusy(){
        if(auto||mixing||(scratch!=null&&scratch.any()))return true;
        for(ExoPlayer deck:decks)if(deck.getPlayWhenReady()||deck.isPlaying())return true;
        for(int[] side:streams)for(int stream:side)if(stream!=0)return true;
        return false;
    }
    boolean samplesActive(){return ownedSamples!=null&&ownedSamples.active();}
    private int loadSample(String path){return ownedSamples!=null?ownedSamples.load(path):pool.load(path,1);}
    private void unloadSample(int id){if(ownedSamples!=null)ownedSamples.unload(id);else pool.unload(id);}
    private int playSample(int id,float volume,boolean loop){return ownedSamples!=null?ownedSamples.play(id,volume,loop):pool.play(id,volume,volume,1,loop?-1:0,1);}
    private void stopSample(int id){if(ownedSamples!=null)ownedSamples.stop(id);else pool.stop(id);}
    private void volumeSample(int id,float volume){if(ownedSamples!=null)ownedSamples.volume(id,volume);else pool.setVolume(id,volume,volume);}
    private void loopSample(int id,boolean loop){if(ownedSamples!=null)ownedSamples.loop(id,loop);else pool.setLoop(id,loop?-1:0);}
    void sampleVolume(int side,float volume){samplerVolume[side]=volume;prefs.edit().putFloat("sample-volume-"+side,volume).apply();for(int stream:streams[side])if(stream!=0)volumeSample(stream,volume);}
    void assign(int side,int pad,String path,String name){
        assign(side,pad,path,name,loops[side][pad]);
    }
    void assign(int side,int pad,String path,String name,boolean loop){
        SamplerStore.save(prefs,side,pad,path,name,loop);
        stopPad(side,pad);int old=sampleIds[side][pad];if(old!=0){unloadSample(old);ready.remove(old);}
        samples[side][pad]=path;names[side][pad]=name;loops[side][pad]=loop;sampleIds[side][pad]=loadSample(path);message="Effect assigned. Tap its pad to play.";
    }
    void rename(int side,int pad,String name){SamplerStore.save(prefs,side,pad,samples[side][pad],name,loops[side][pad]);names[side][pad]=name;}
    void trigger(int side,int pad){int id=sampleIds[side][pad];if(!ready.contains(id)){message="Load a short effect first, or wait for decoding.";return;}PlaybackService.claimDj();stopPad(side,pad);streams[side][pad]=playSample(id,samplerVolume[side],loops[side][pad]);}
    boolean playerRepeats(){return autoRules.optBoolean("smart")&&!autoRules.optString("fallback","relax_recent").equals("stop");}
    long playerPosition(){return decks[fader<.5?0:1].getCurrentPosition();}
    List<MediaItem> playerQueue(){
        List<MediaItem> result=new ArrayList<>();MediaItem current=decks[fader<.5?0:1].getCurrentMediaItem();
        if(current!=null)result.add(current);
        for(int i=nextAuto;i<autoQueue.size();i++){MediaItem item=autoQueue.get(i);if(i!=nextAuto||current==null||!item.mediaId.equals(current.mediaId))result.add(item);}
        if(playerRepeats())for(MediaItem item:autoSource){boolean included=false;for(MediaItem existing:result)if(existing.mediaId.equals(item.mediaId)){included=true;break;}if(!included)result.add(item);}
        return result;
    }
    void pauseForPlayer(){if(scratch!=null)scratch.pauseAll();auto(false);for(ExoPlayer deck:decks)deck.pause();for(int side=0;side<2;side++)for(int pad=0;pad<9;pad++)stopPad(side,pad);}
    void stopPad(int side,int pad){if(streams[side][pad]!=0)stopSample(streams[side][pad]);streams[side][pad]=0;}
    void clearPad(int side,int pad){SamplerStore.clear(prefs,side,pad);stopPad(side,pad);unloadSample(sampleIds[side][pad]);ready.remove(sampleIds[side][pad]);sampleIds[side][pad]=0;samples[side][pad]="";names[side][pad]="PAD "+(pad+1);}
    void loop(int side,int pad,boolean value){SamplerStore.save(prefs,side,pad,samples[side][pad],names[side][pad],value);loops[side][pad]=value;if(streams[side][pad]!=0)loopSample(streams[side][pad],value);}
    void pauseAll(){if(scratch!=null)scratch.pauseAll();auto=false;handler.removeCallbacks(autoTick);cancelMix();for(ExoPlayer deck:decks)deck.pause();for(int s=0;s<2;s++)for(int p=0;p<9;p++)stopPad(s,p);}
    void prepareScratch(){if(scratch!=null)for(int deck=0;deck<4;deck++)scratch.prepare(deck);}
    boolean beginScratch(int deck){if(scratch==null)return false;if(!scratch.ready(deck))return scratch.begin(deck);if(manager.requestAudioFocus(focus)!=AudioManager.AUDIOFOCUS_REQUEST_GRANTED){message="Audio focus unavailable. Current playback was not changed.";return false;}if(!scratch.begin(deck))return false;cancelMix();return true;}
    long scratchToken(int deck){return scratch==null?0:scratch.token(deck);}
    void dragScratch(int deck,long token,double seconds){if(scratch!=null)scratch.drag(deck,token,seconds,System.nanoTime());}
    void endScratch(int deck,long token,boolean resume){if(scratch!=null)scratch.end(deck,token,resume);}
    void cancelScratches(){if(scratch!=null)scratch.pauseAll();}
    void releaseScratches(){if(scratch!=null)scratch.releaseAll();}
    long scratchPosition(int deck){return scratch==null?decks[deck].getCurrentPosition():scratch.position(deck);}
    void checkpoint(){SharedPreferences.Editor edit=prefs.edit();for(int i=0;i<4;i++){MediaItem item=decks[i].getCurrentMediaItem();if(item!=null&&item.localConfiguration!=null)edit.putString("prepared-uri-"+i,item.localConfiguration.uri.toString()).putString("prepared-title-"+i,String.valueOf(item.mediaMetadata.title)).putString("prepared-id-"+i,item.mediaId).putLong("prepared-position-"+i,decks[i].getCurrentPosition()).putString("prepared-metadata-"+i,preparation(item).toString());else edit.remove("prepared-uri-"+i).remove("prepared-title-"+i).remove("prepared-id-"+i).remove("prepared-position-"+i);}edit.apply();}
    void close(PlaybackService service){closed=true;if(scratch!=null)scratch.close();pauseAll();checkpoint();manager.abandonAudioFocusRequest(focus);for(MediaSession session:sessions){service.removeSession(session);session.release();}for(ExoPlayer deck:decks)deck.release();if(pool!=null)pool.release();if(ownedSamples!=null)ownedSamples.close();}
}
