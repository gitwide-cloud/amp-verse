package cloud.gitwide.ampverse;

import androidx.media3.common.Player;

/** Local listener state only; stopping never changes the selected server program. */
final class PhoneListening {
  private PhoneListening() {}

  static boolean active(Player player, boolean radio) {
    if (player == null || !radio || !player.getPlayWhenReady()) return false;
    int state = player.getPlaybackState();
    // stop() retains playWhenReady. IDLE/ENDED are not an active listening request.
    return state == Player.STATE_BUFFERING || state == Player.STATE_READY;
  }

  static void stop(Player player) {
    // Clear the request before stopping, so subsequent prepare/reconnect cannot resume it.
    player.pause();
    player.stop();
  }
}
