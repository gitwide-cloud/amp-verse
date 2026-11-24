package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import androidx.media3.common.Player;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.List;
import org.junit.Test;

public final class PhoneListeningTest {
  private static Player player(int state, boolean ready, List<String> calls) {
    return (Player)
        Proxy.newProxyInstance(
            Player.class.getClassLoader(),
            new Class<?>[] {Player.class},
            (proxy, method, args) -> {
              if (method.getName().equals("getPlaybackState")) return state;
              if (method.getName().equals("getPlayWhenReady")) return ready;
              if (method.getName().equals("pause") || method.getName().equals("stop")) {
                calls.add(method.getName());
                return null;
              }
              throw new AssertionError("Unexpected player call: " + method.getName());
            });
  }

  @Test
  public void stoppedAndEndedRequestsAreNotListening() {
    List<String> calls = new ArrayList<>();
    assertFalse(PhoneListening.active(player(Player.STATE_IDLE, true, calls), true));
    assertFalse(PhoneListening.active(player(Player.STATE_ENDED, true, calls), true));
    assertFalse(PhoneListening.active(null, true));
    assertTrue(calls.isEmpty());
  }

  @Test
  public void readyAndBufferingRequireRadioAndPlayRequest() {
    for (int state : new int[] {Player.STATE_READY, Player.STATE_BUFFERING}) {
      List<String> calls = new ArrayList<>();
      assertTrue(PhoneListening.active(player(state, true, calls), true));
      assertFalse(PhoneListening.active(player(state, false, calls), true));
      assertFalse(PhoneListening.active(player(state, true, calls), false));
      assertTrue(calls.isEmpty());
    }
  }

  @Test
  public void stopClearsPlayRequestBeforeStoppingWithoutOtherCommands() {
    List<String> calls = new ArrayList<>();
    PhoneListening.stop(player(Player.STATE_READY, true, calls));
    assertEquals(java.util.Arrays.asList("pause", "stop"), calls);
  }
}
