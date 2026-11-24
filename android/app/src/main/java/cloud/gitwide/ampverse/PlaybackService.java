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
    private static volatile PlaybackService instance;
    private DjEngine dj;
    private volatile OwnedAudioOutput ownedOutput;
    private volatile PhoneMasterBroadcast phoneBroadcast;
    private volatile boolean phoneSourceAllowed;
    private volatile long phoneSourceVersion;
    private volatile String phoneTargetBase="";
    static OwnedAudioOutput ownedOutput(){return instance==null?null:instance.ownedOutput;}
    static boolean phoneSending(){return instance!=null&&instance.phoneBroadcast!=null&&instance.phoneBroadcast.active();}
    static String phoneBroadcastStatus(){if(android.os.Build.VERSION.SDK_INT<30)return "Phone master sending requires Android 11 or newer; station listening stays available.";return instance==null||instance.phoneBroadcast==null?"Phone master sending is not enabled in this build":instance.phoneBroadcast.status();}
    static boolean phoneSenderAvailable(){return instance!=null&&instance.phoneBroadcast!=null&&instance.ownedOutput.ready()&&phoneEncoderAvailable();}
    private static boolean phoneEncoderAvailable(){if(android.os.Build.VERSION.SDK_INT<30)return false;try{Class.forName("cloud.gitwide.ampverse.FullPhoneMp3Encoder",false,PlaybackService.class.getClassLoader());return true;}catch(ClassNotFoundException unavailable){return false;}}
    static void stopPhoneBroadcast(){if(instance!=null&&instance.phoneBroadcast!=null)instance.phoneBroadcast.stop();}
    static void phoneSourceChanged(){if(instance!=null)instance.notePhoneSource();}
    private static boolean ampReceiver(androidx.media3.common.MediaItem item){
        return item!=null&&item.localConfiguration!=null&&PhoneSourceIdentity.receiver(item.localConfiguration.uri.toString(),instance==null?"":instance.phoneTargetBase);
    }
    private void notePhoneSource(){
        phoneSourceVersion++;
        boolean allowed=djOwner?(dj!=null&&!ampReceiver(dj.decks[0].getCurrentMediaItem())&&!ampReceiver(dj.decks[1].getCurrentMediaItem())):session!=null&&!ampReceiver(session.getPlayer().getCurrentMediaItem());
        phoneSourceAllowed=allowed;if(!allowed&&phoneBroadcast!=null)phoneBroadcast.stop();
    }
    static String startPhoneBroadcast(CastClient selected){
        PlaybackService service=instance;
        if(service==null||service.phoneBroadcast==null||!service.ownedOutput.ready()||!phoneEncoderAvailable())return "Phone master sending needs the qualified Full audio path; current playback is unchanged.";
        if(service.phoneBroadcast.active())return "Stop the current phone sender before choosing another source.";
        service.notePhoneSource();
        if(!service.phoneSourceAllowed)return "This phone is receiving an AMP station. Choose a local Player or DJ source before sending; playback is unchanged.";
        boolean loaded=service.djOwner?service.dj!=null&&(service.dj.active(0)||service.dj.active(1)||service.dj.samplesActive()):service.session!=null&&service.session.getPlayer().getPlayWhenReady()&&service.session.getPlayer().getPlaybackState()!=Player.STATE_IDLE&&service.session.getPlayer().getPlaybackState()!=Player.STATE_ENDED&&service.session.getPlayer().getPlayerError()==null;
        if(!loaded||!service.ownedOutput.mixer.hasOwnedPcm())return "Start this phone’s Player, DJ mix or pad, then send it after audio begins.";
        final String[] result={"Preparing this phone’s master broadcast"};
        selected.withLiveAuthorization((base,password,server,station,library)->{
            try{service.phoneTargetBase=base;service.notePhoneSource();if(!service.phoneSourceAllowed){result[0]="This phone is receiving the selected AMP station. Choose local Player music or DJ before sending; playback is unchanged.";return;}PhoneLiveSession.Identity identity=new PhoneLiveSession.Identity(server,station,library);PhoneLiveSession live=new PhoneLiveSession(identity,new PhoneLiveHttpTransport(base,password,identity));final long version=service.phoneSourceVersion;
                service.phoneBroadcast.start(live,()->instance==service&&service.phoneSourceAllowed&&service.phoneSourceVersion==version&&service.ownedOutput.mixer.hasOwnedPcm(),()->instance==service&&service.phoneSourceAllowed&&service.ownedOutput.ready());
            }catch(Exception unavailable){result[0]="Phone broadcast could not start. Verify the selected host and stop any existing phone sender; playback is unchanged.";}
        });return result[0];
    }
    static void suppressOwnedOutput(boolean suppressed){OwnedAudioOutput output=ownedOutput();if(output!=null)output.suppress(suppressed);if(suppressed)stopPhoneBroadcast();}
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
    static void claimDj(){if(instance!=null){instance.djOwner=true;instance.notePhoneSource();suppressOwnedOutput(false);if(instance.ownedOutput!=null)instance.ownedOutput.mixer.owner(OwnedPcmMixer.Owner.DJ);if(instance.session!=null)instance.session.getPlayer().pause();}}
    static void claimDjForScratch(Runnable afterPause){
        PlaybackService service=instance;
        if(service==null||service.session==null){afterPause.run();return;}
        service.djOwner=true;service.notePhoneSource();suppressOwnedOutput(false);if(service.ownedOutput!=null)service.ownedOutput.mixer.owner(OwnedPcmMixer.Owner.DJ);Player player=service.session.getPlayer();player.pause();
        if(player instanceof ExoPlayer) ((ExoPlayer)player).createMessage((type,payload)->service.handler.post(afterPause)).send();
        else service.handler.post(afterPause);
    }
    private void claimPlayer(){suppressOwnedOutput(false);if(ownedOutput!=null)ownedOutput.mixer.owner(OwnedPcmMixer.Owner.PLAYER);if(djOwner){djOwner=false;if(dj!=null)dj.pauseForPlayer();}notePhoneSource();}
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
        // Candidate cutover stays off until sampler, scratch and handset acceptance pass.
        if(getSharedPreferences("settings",MODE_PRIVATE).getBoolean("owned-pcm-master-candidate",false))ownedOutput=new OwnedAudioOutput();
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
            @Override public void onPlayWhenReadyChanged(boolean ready,int reason){if(ready)claimPlayer();else{if(reason==Player.PLAY_WHEN_READY_CHANGE_REASON_AUDIO_FOCUS_LOSS||reason==Player.PLAY_WHEN_READY_CHANGE_REASON_AUDIO_BECOMING_NOISY)suppressOwnedOutput(true);cancelRadioRetry();bufferingSince=0;hlsAttemptSince=0;}}
            @Override public void onMediaItemTransition(androidx.media3.common.MediaItem item,int reason){
                notePhoneSource();bufferingSince=0;lastLiveCatchup=0;
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
        if(ownedOutput!=null)phoneBroadcast=new PhoneMasterBroadcast(ownedOutput.mixer,()->{
            try{return (PhoneMasterBroadcast.Encoder)Class.forName("cloud.gitwide.ampverse.FullPhoneMp3Encoder").getDeclaredConstructor(android.content.Context.class).newInstance(this);}
            catch(ReflectiveOperationException|LinkageError unavailable){throw new java.io.IOException("Full phone encoder unavailable",unavailable);}
        },text->{});
        notePhoneSource();handler.post(fades);handler.post(liveWatchdog);
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
        return renderers(context,playerFade,playerFade?OwnedPcmMixer.Source.PLAYER:OwnedPcmMixer.Source.PRIVATE_CUE);
    }
    static androidx.media3.exoplayer.DefaultRenderersFactory renderers(android.content.Context context,boolean playerFade,OwnedPcmMixer.Source source) {
        androidx.media3.exoplayer.DefaultRenderersFactory renderers=new androidx.media3.exoplayer.DefaultRenderersFactory(context){
            @Override protected androidx.media3.exoplayer.audio.AudioSink buildAudioSink(android.content.Context context,boolean floating,boolean playbackParameters){
                OwnedAudioOutput owned=ownedOutput();
                if(owned!=null&&source!=OwnedPcmMixer.Source.PRIVATE_CUE){
                    try{owned.prepare();return new OwnedMediaAudioSink(owned,source,playerFade);}
                    catch(java.io.IOException unavailable){owned.disqualify();android.util.Log.w("AMPVerse","Owned output unavailable; preserving normal playback");}
                }
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
        if(phoneBroadcast!=null){phoneBroadcast.close();phoneBroadcast=null;}
        instance=null;
        if(dj!=null){dj.close(this);dj=null;}
        if(settings!=null)settings.unregisterOnSharedPreferenceChangeListener(changed);
        if (session != null) { session.getPlayer().release(); session.release(); }
        if(ownedOutput!=null){ownedOutput.close();ownedOutput=null;}
        super.onDestroy();
    }
}
