package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;
public class VinylReleaseTest {
    @Test public void navigationAndTouchCancelRestoreCapturedPlayingState(){VinylRelease playing=new VinylRelease(true);assertTrue(playing.stop(true));assertTrue(playing.restore());assertTrue(playing.play());assertFalse(playing.stop(true));assertTrue(playing.play());}
    @Test public void pausedTouchCannotAutoplayOnRelease(){VinylRelease paused=new VinylRelease(false);paused.stop(true);assertTrue(paused.restore());assertFalse(paused.play());}
    @Test public void focusLossWinsEvenAfterReleaseAndLateDetachCannotResurrect(){VinylRelease playing=new VinylRelease(true);playing.stop(true);assertTrue(playing.play());playing.stop(false);assertFalse(playing.restore());playing.stop(true);assertFalse(playing.play());VinylRelease cancelled=new VinylRelease(true);cancelled.stop(false);cancelled.stop(true);assertFalse(cancelled.play());}
}
