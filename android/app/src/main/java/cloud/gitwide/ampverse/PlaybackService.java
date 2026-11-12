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
    private final android.os.Handler handler=new android.os.Handler(android.os.Looper.getMainLooper());
    static void claimDj(){if(instance!=null){instance.djOwner=true;if(instance.session!=null)instance.session.getPlayer().pause();}}
    private void claimPlayer(){if(djOwner){djOwner=false;if(dj!=null)dj.pauseForPlayer();}}
    static void switchToPlayer(){if(instance!=null)instance.adoptDjPlaylist();}
    static String owner(){return instance!=null&&instance.djOwner?"dj":"player";}
    private void adoptDjPlaylist(){
        if(!djOwner||dj==null||session==null)return;
        java.util.List<androidx.media3.common.MediaItem> queue=dj.playerQueue();
        long position=dj.playerPosition();
        if(queue.isEmpty())return;
        boolean repeat=dj.playerRepeats();claimPlayer();
        Player player=session.getPlayer();player.setRepeatMode(repeat?Player.REPEAT_MODE_ALL:Player.REPEAT_MODE_OFF);player.setShuffleModeEnabled(false);player.setMediaItems(queue,0,position);player.prepare();player.play();
    }
    private final Runnable fades=new Runnable(){public void run(){
        if(session!=null&&!djOwner){Player p=session.getPlayer();long duration=p.getDuration(),at=p.getCurrentPosition();
            androidx.media3.common.MediaItem item=p.getCurrentMediaItem();
            boolean radio=item!=null&&item.mediaId.equals("amp-radio");
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
        ExoPlayer player = new ExoPlayer.Builder(this,renderers(this,true)).build();
        player.setAudioAttributes(new AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(), true);
        player.setHandleAudioBecomingNoisy(true);
        player.setWakeMode(C.WAKE_MODE_LOCAL);
        try { player.setMediaItems(LibraryStore.read(this)); }
        catch (IllegalStateException error) { android.util.Log.e("AMPVerse", "Library load failed", error); }
        player.setRepeatMode(getSharedPreferences("settings", MODE_PRIVATE).getBoolean("repeat", false) ? Player.REPEAT_MODE_ALL : Player.REPEAT_MODE_OFF);
        player.setShuffleModeEnabled(settings.getBoolean("shuffle",false));
        player.addListener(new Player.Listener(){@Override public void onPlayWhenReadyChanged(boolean ready,int reason){if(ready)claimPlayer();}@Override public void onIsPlayingChanged(boolean playing){if(!playing){AudioDsp.spectrum=new float[24];AudioDsp.spectrumTime=0;}}});
        session = new MediaSession.Builder(this, player).setSessionActivity(PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT)).build();
        handler.post(fades);
    }
    static androidx.media3.exoplayer.DefaultRenderersFactory renderers(android.content.Context context) {return renderers(context,false);}
    static androidx.media3.exoplayer.DefaultRenderersFactory renderers(android.content.Context context,boolean playerFade) {
        androidx.media3.exoplayer.DefaultRenderersFactory renderers=new androidx.media3.exoplayer.DefaultRenderersFactory(context){
            @Override protected androidx.media3.exoplayer.audio.AudioSink buildAudioSink(android.content.Context context,boolean floating,boolean playbackParameters){
                return new androidx.media3.exoplayer.audio.DefaultAudioSink.Builder(context).setEnableFloatOutput(false).setAudioProcessors(new androidx.media3.common.audio.AudioProcessor[]{new AudioDsp(playerFade)}).build();
            }
        };
        return renderers;
    }
    @Override public MediaSession onGetSession(MediaSession.ControllerInfo info) { return session; }
    @Override public void onDestroy() {
        handler.removeCallbacks(fades);
        instance=null;
        if(dj!=null){dj.close(this);dj=null;}
        if(settings!=null)settings.unregisterOnSharedPreferenceChangeListener(changed);
        if (session != null) { session.getPlayer().release(); session.release(); }
        super.onDestroy();
    }
}
