package cloud.gitwide.ampverse;

import android.app.PendingIntent;
import android.content.Intent;
import androidx.media3.common.AudioAttributes;
import androidx.media3.common.C;
import androidx.media3.common.Player;
import androidx.media3.exoplayer.ExoPlayer;
import androidx.media3.session.MediaSession;
import androidx.media3.session.MediaSessionService;

/** Playback and platform notification/lock-screen controls outlive the activity. */
@androidx.annotation.OptIn(markerClass = androidx.media3.common.util.UnstableApi.class)
public final class PlaybackService extends MediaSessionService {
    private MediaSession session;
    private static PlaybackService instance;
    private DjEngine dj;
    private boolean djOwner;
    static Player player(){return instance==null||instance.session==null?null:instance.session.getPlayer();}
    static DjEngine existingDj(){return instance==null?null:instance.dj;}
    static void pauseForLab(){if(instance!=null){instance.claimPlayer();if(instance.dj!=null)instance.dj.pauseForPlayer();if(instance.session!=null)instance.session.getPlayer().pause();}}
    private Runnable radioRetry;
    private int radioAttempts;
    private long bufferingSince,lastLiveCatchup,hlsAttemptSince;
    private String hlsAttemptUri="";
    private boolean hlsReady;
    private final android.os.Handler handler=new android.os.Handler(android.os.Looper.getMainLooper());
    static void claimDj(){if(instance!=null){instance.djOwner=true;if(instance.session!=null)instance.session.getPlayer().pause();}}
    static void claimDjForScratch(Runnable afterPause){
        PlaybackService service=instance;
        if(service==null||service.session==null){afterPause.run();return;}
        service.djOwner=true;Player player=service.session.getPlayer();player.pause();
        if(player instanceof ExoPlayer) ((ExoPlayer)player).createMessage((type,payload)->service.handler.post(afterPause)).send();
        else service.handler.post(afterPause);
    }
    private void claimPlayer(){if(djOwner){djOwner=false;if(dj!=null)dj.pauseForPlayer();}}
    static void switchToPlayer(){if(instance!=null)instance.adoptDjPlaylist();}
    static String owner(){return instance!=null&&instance.djOwner?"dj":"player";}
    private void adoptDjPlaylist(){
        if(!djOwner||dj==null||session==null)return;
        java.util.List<androidx.media3.common.MediaItem> queue=dj.playerQueue();
        long position=dj.playerPosition();
        if(queue.isEmpty()){claimPlayer();return;}
        boolean repeat=dj.playerRepeats();claimPlayer();
        Player player=session.getPlayer();player.setRepeatMode(repeat?Player.REPEAT_MODE_ALL:Player.REPEAT_MODE_OFF);player.setShuffleModeEnabled(false);player.setMediaItems(queue,0,position);player.prepare();player.play();
    }
    private final Runnable fades=new Runnable(){public void run(){
        if(session!=null&&!djOwner){Player p=session.getPlayer();long duration=p.getDuration(),at=p.getCurrentPosition();
            androidx.media3.common.MediaItem item=p.getCurrentMediaItem();
            boolean radio=item!=null&&(item.mediaId.equals("amp-radio")||item.mediaId.startsWith("amp-podcast:"));
            float seconds=Math.max(1,getSharedPreferences("dj",MODE_PRIVATE).getInt("fade-seconds",3));
            float envelope=radio||duration<=0?1f:Math.min(Math.min(1f,at/(seconds*1000)),Math.max(0f,(duration-at)/(seconds*1000)));
            AudioDsp.playerEnvelope=envelope;
        }handler.postDelayed(this,40);
    }};
    static DjEngine dj(){if(instance==null)return null;if(instance.dj==null)instance.dj=new DjEngine(instance);return instance.dj;}
    private android.content.SharedPreferences settings;
    private final android.content.SharedPreferences.OnSharedPreferenceChangeListener changed=(prefs,key)->{
        if(key!=null&&key.startsWith("eq-"))AudioDsp.load(prefs);
        if(session!=null&&"repeat".equals(key))session.getPlayer().setRepeatMode(prefs.getBoolean("repeat",false)?Player.REPEAT_MODE_ALL:Player.REPEAT_MODE_OFF);
        if(session!=null&&"shuffle".equals(key))session.getPlayer().setShuffleModeEnabled(prefs.getBoolean("shuffle",false));
    };
    @Override public void onCreate() {
        super.onCreate();
        instance=this;
        settings=getSharedPreferences("settings",MODE_PRIVATE);AudioDsp.load(settings);settings.registerOnSharedPreferenceChangeListener(changed);
        ExoPlayer player = new ExoPlayer.Builder(this,renderers(this,true))
            .setMediaSourceFactory(new StationMediaSourceFactory(this))
            .setBandwidthMeter(new androidx.media3.exoplayer.upstream.DefaultBandwidthMeter.Builder(this).setInitialBitrateEstimate(220000).build())
            .setTrackSelector(new androidx.media3.exoplayer.trackselection.DefaultTrackSelector(this,new androidx.media3.exoplayer.trackselection.AdaptiveTrackSelection.Factory(2500,1500,2500,.7f)))
            .setLoadControl(new androidx.media3.exoplayer.DefaultLoadControl.Builder().setBufferDurationsMs(1500,6000,1000,1500).setPrioritizeTimeOverSizeThresholds(true).build()).build();
        player.setAudioAttributes(new AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(), true);
        player.setHandleAudioBecomingNoisy(true);
        player.setWakeMode(C.WAKE_MODE_LOCAL);
        try { player.setMediaItems(LibraryStore.read(this)); }
        catch (IllegalStateException error) { android.util.Log.e("AMPVerse", "Library load failed", error); }
        player.setRepeatMode(getSharedPreferences("settings", MODE_PRIVATE).getBoolean("repeat", false) ? Player.REPEAT_MODE_ALL : Player.REPEAT_MODE_OFF);
        player.setShuffleModeEnabled(settings.getBoolean("shuffle",false));
        player.addListener(new Player.Listener(){
            @Override public void onPlayWhenReadyChanged(boolean ready,int reason){if(ready)claimPlayer();else{cancelRadioRetry();bufferingSince=0;hlsAttemptSince=0;}}
            @Override public void onMediaItemTransition(androidx.media3.common.MediaItem item,int reason){
                bufferingSince=0;lastLiveCatchup=0;
                String selected=item!=null&&item.localConfiguration!=null?item.localConfiguration.uri.toString():"";
                if(!selected.equals(hlsAttemptUri)){hlsAttemptUri=selected;hlsReady=false;hlsAttemptSince=android.os.SystemClock.elapsedRealtime();}
                player.setWakeMode(isRadio(item)?C.WAKE_MODE_NETWORK:C.WAKE_MODE_LOCAL);
                if(!isRadio(item)){cancelRadioRetry();radioAttempts=0;}
            }
            @Override public void onPlayerError(androidx.media3.common.PlaybackException error){retryRadio(player);}
            @Override public void onPlaybackStateChanged(int state){bufferingSince=state==Player.STATE_BUFFERING?android.os.SystemClock.elapsedRealtime():0;if(state==Player.STATE_ENDED)retryRadio(player);}
            @Override public void onIsPlayingChanged(boolean playing){if(playing){hlsReady=true;hlsAttemptSince=0;radioAttempts=0;cancelRadioRetry();}else{AudioDsp.spectrum=new float[24];AudioDsp.spectrumTime=0;}}
        });
        player.addAnalyticsListener(new androidx.media3.exoplayer.analytics.AnalyticsListener(){
            @Override public void onDownstreamFormatChanged(EventTime eventTime,androidx.media3.exoplayer.source.MediaLoadData data){
                if(data.trackFormat!=null){int bitrate=data.trackFormat.averageBitrate>0?data.trackFormat.averageBitrate:data.trackFormat.peakBitrate;
                    if(bitrate>0)StationMediaSourceFactory.report(player.getCurrentMediaItem(),"Adaptive AAC · "+(bitrate/1000)+" kbps");}
            }
        });
        session = new MediaSession.Builder(this, player).setSessionActivity(PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT)).build();
        handler.post(fades);handler.post(liveWatchdog);
    }
    private static boolean isRadio(androidx.media3.common.MediaItem item){return item!=null&&item.mediaId.equals("amp-radio");}
    private void cancelRadioRetry(){if(radioRetry!=null){handler.removeCallbacks(radioRetry);radioRetry=null;}}
    private void retryRadio(ExoPlayer player){retryRadio(player,false);}
    private void retryRadio(ExoPlayer player,boolean stalled){
        androidx.media3.common.MediaItem item=player.getCurrentMediaItem();
        if(!mayRecover(player)||radioRetry!=null)return;
        long delay=Math.min(6000,500L<<Math.min(4,radioAttempts++));
        radioRetry=()->{
            radioRetry=null;
            if(mayRecover(player)&&item.equals(player.getCurrentMediaItem())&&(player.getPlayerError()!=null||player.getPlaybackState()==Player.STATE_ENDED||(stalled&&player.getPlaybackState()==Player.STATE_BUFFERING))){
                if(hlsReady){hlsReady=false;hlsAttemptSince=android.os.SystemClock.elapsedRealtime();}
                StationMediaSourceFactory.report(item,"Adaptive AAC · reconnecting to live audio");
                player.setMediaItem(item);player.prepare();
            }
        };
        handler.postDelayed(radioRetry,delay);
    }
    private boolean mayRecover(ExoPlayer player){return LiveRecoveryPolicy.allowed(isRadio(player.getCurrentMediaItem()),player.getPlayWhenReady(),player.getPlaybackSuppressionReason()!=Player.PLAYBACK_SUPPRESSION_REASON_NONE,djOwner);}
    private final Runnable liveWatchdog=new Runnable(){public void run(){
        if(session!=null){
            ExoPlayer player=(ExoPlayer)session.getPlayer();
            androidx.media3.common.MediaItem item=player.getCurrentMediaItem();
            long now=android.os.SystemClock.elapsedRealtime();
            boolean hls=item!=null&&item.localConfiguration!=null&&StationMediaSourceFactory.adaptive(item.localConfiguration.uri);
            if(hls&&!hlsReady&&mayRecover(player)&&hlsAttemptSince==0)hlsAttemptSince=now;
            if(hls&&!hlsReady&&mayRecover(player)&&LiveRecoveryPolicy.preparationExpired(hlsAttemptSince,now)){
                android.net.Uri selected=item.localConfiguration.uri;
                android.net.Uri legacy=selected.buildUpon().path("/amp-protected.mp3").clearQuery()
                    .appendQueryParameter("server",selected.getQueryParameter("server")).appendQueryParameter("station",selected.getQueryParameter("station")).build();
                StationMediaSourceFactory.report(item,"Adaptive AAC unavailable · using protected MP3");
                cancelRadioRetry();
                player.setMediaItem(item.buildUpon().setUri(legacy).setMimeType(androidx.media3.common.MimeTypes.AUDIO_MPEG)
                    .setLiveConfiguration(new androidx.media3.common.MediaItem.LiveConfiguration.Builder().build()).build());player.prepare();
            }else if(LiveRecoveryPolicy.retryError(mayRecover(player),player.getPlayerError()!=null,player.getPlaybackState()==Player.STATE_ENDED)){
                // A retry timer may have elapsed while focus was suppressed.
                // Reconsider the still-requested stream after focus returns.
                retryRadio(player);
            }else if(mayRecover(player)&&item!=null&&item.localConfiguration!=null&&(StationMediaSourceFactory.adaptive(item.localConfiguration.uri)||ProtectedCastSource.accepts(item.localConfiguration.uri))){
                if(player.getPlaybackState()==Player.STATE_BUFFERING){
                    if(bufferingSince==0)bufferingSince=now;
                    if(LiveRecoveryPolicy.stalled(bufferingSince,now))retryRadio(player,true);
                }else{
                    bufferingSince=0;
                    long offset=player.getCurrentLiveOffset();
                    if(player.isPlaying()&&offset!=C.TIME_UNSET&&LiveRecoveryPolicy.behind(offset,now,lastLiveCatchup)){
                        lastLiveCatchup=now;StationMediaSourceFactory.report(item,"Adaptive AAC · catching up to live audio");player.seekToDefaultPosition();
                    }
                }
            }else{bufferingSince=0;if(!mayRecover(player))hlsAttemptSince=0;}
        }
        handler.postDelayed(this,500);
    }};
    static androidx.media3.exoplayer.DefaultRenderersFactory renderers(android.content.Context context) {return renderers(context,false);}
    static androidx.media3.exoplayer.DefaultRenderersFactory renderers(android.content.Context context,boolean playerFade) {
        androidx.media3.exoplayer.DefaultRenderersFactory renderers=new androidx.media3.exoplayer.DefaultRenderersFactory(context){
            @Override protected androidx.media3.exoplayer.audio.AudioSink buildAudioSink(android.content.Context context,boolean floating,boolean playbackParameters){
                return new androidx.media3.exoplayer.audio.DefaultAudioSink.Builder(context).setEnableFloatOutput(false).setAudioProcessors(new androidx.media3.common.audio.AudioProcessor[]{new AudioDsp(playerFade)}).build();
            }
        };
        return renderers;
    }
    @Override public MediaSession onGetSession(MediaSession.ControllerInfo info) {
        if(!info.isTrusted()&&getPackageManager().checkSignatures(info.getUid(),android.os.Process.myUid())!=android.content.pm.PackageManager.SIGNATURE_MATCH)return null;
        return session;
    }
    @Override public void onDestroy() {
        handler.removeCallbacks(fades);handler.removeCallbacks(liveWatchdog);cancelRadioRetry();
        instance=null;
        if(dj!=null){dj.close(this);dj=null;}
        if(settings!=null)settings.unregisterOnSharedPreferenceChangeListener(changed);
        if (session != null) { session.getPlayer().release(); session.release(); }
        super.onDestroy();
    }
}
