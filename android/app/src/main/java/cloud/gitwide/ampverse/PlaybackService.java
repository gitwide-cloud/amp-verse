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
    private android.content.SharedPreferences settings;
    private final android.content.SharedPreferences.OnSharedPreferenceChangeListener changed=(prefs,key)->{if(key!=null&&key.startsWith("eq-"))AudioDsp.load(prefs);};
    @Override public void onCreate() {
        super.onCreate();
        settings=getSharedPreferences("settings",MODE_PRIVATE);AudioDsp.load(settings);settings.registerOnSharedPreferenceChangeListener(changed);
        androidx.media3.exoplayer.DefaultRenderersFactory renderers=new androidx.media3.exoplayer.DefaultRenderersFactory(this){
            @Override protected androidx.media3.exoplayer.audio.AudioSink buildAudioSink(android.content.Context context,boolean floating,boolean playbackParameters){
                return new androidx.media3.exoplayer.audio.DefaultAudioSink.Builder(context).setEnableFloatOutput(false).setAudioProcessors(new androidx.media3.common.audio.AudioProcessor[]{new AudioDsp()}).build();
            }
        };
        ExoPlayer player = new ExoPlayer.Builder(this,renderers).build();
        player.setAudioAttributes(new AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(), true);
        player.setHandleAudioBecomingNoisy(true);
        player.setWakeMode(C.WAKE_MODE_LOCAL);
        try { player.setMediaItems(LibraryStore.read(this)); }
        catch (IllegalStateException error) { android.util.Log.e("AMPVerse", "Library load failed", error); }
        player.setRepeatMode(getSharedPreferences("settings", MODE_PRIVATE).getBoolean("repeat", false) ? Player.REPEAT_MODE_ALL : Player.REPEAT_MODE_OFF);
        player.setShuffleModeEnabled(settings.getBoolean("shuffle",false));
        player.addListener(new Player.Listener(){@Override public void onIsPlayingChanged(boolean playing){if(!playing){AudioDsp.spectrum=new float[24];AudioDsp.spectrumTime=0;}}});
        session = new MediaSession.Builder(this, player).setSessionActivity(PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT)).build();
    }
    @Override public MediaSession onGetSession(MediaSession.ControllerInfo info) { return session; }
    @Override public void onDestroy() {
        if(settings!=null)settings.unregisterOnSharedPreferenceChangeListener(changed);
        if (session != null) { session.getPlayer().release(); session.release(); }
        super.onDestroy();
    }
}
