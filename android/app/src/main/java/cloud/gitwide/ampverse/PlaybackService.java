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
public final class PlaybackService extends MediaSessionService {
    private MediaSession session;
    @Override public void onCreate() {
        super.onCreate();
        ExoPlayer player = new ExoPlayer.Builder(this).build();
        player.setAudioAttributes(new AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(), true);
        player.setHandleAudioBecomingNoisy(true);
        player.setWakeMode(C.WAKE_MODE_LOCAL);
        try { player.setMediaItems(LibraryStore.read(this)); }
        catch (IllegalStateException error) { android.util.Log.e("AMPVerse", "Library load failed", error); }
        player.setRepeatMode(getSharedPreferences("settings", MODE_PRIVATE).getBoolean("repeat", false) ? Player.REPEAT_MODE_ALL : Player.REPEAT_MODE_OFF);
        session = new MediaSession.Builder(this, player).setSessionActivity(PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT)).build();
    }
    @Override public MediaSession onGetSession(MediaSession.ControllerInfo info) { return session; }
    @Override public void onDestroy() {
        if (session != null) { session.getPlayer().release(); session.release(); }
        super.onDestroy();
    }
}
