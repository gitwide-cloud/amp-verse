package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import androidx.media3.common.Player;
import androidx.media3.exoplayer.ExoPlayer;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;

@RunWith(AndroidJUnit4.class)
public final class PhoneListeningDeviceTest {
  @Test
  public void realExoIdleRetainsRequestButIsNotListeningAndStopClearsIt() {
    // Independent, silent player: no service, user queue, preferences or network touched.
    InstrumentationRegistry.getInstrumentation()
        .runOnMainSync(
            () -> {
              ExoPlayer player =
                  new ExoPlayer.Builder(
                          InstrumentationRegistry.getInstrumentation().getTargetContext())
                      .build();
              try {
                player.setVolume(0);
                player.setPlayWhenReady(true);
                player.stop();
                assertEquals(Player.STATE_IDLE, player.getPlaybackState());
                assertTrue(player.getPlayWhenReady());
                assertFalse(PhoneListening.active(player, true));
                PhoneListening.stop(player);
                assertFalse(player.getPlayWhenReady());
                assertEquals(Player.STATE_IDLE, player.getPlaybackState());
                assertFalse(PhoneListening.active(player, true));
              } finally {
                player.release();
              }
            });
  }
}
