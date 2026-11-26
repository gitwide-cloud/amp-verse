package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;
public class VinylReleaseTest {
    @Test public void navigationAndTouchCancelRestoreCapturedPlayingState(){VinylRelease playing=new VinylRelease(true);assertTrue(playing.stop(true));assertTrue(playing.restore());assertTrue(playing.play());assertFalse(playing.stop(true));assertTrue(playing.play());}
    @Test public void pausedTouchCannotAutoplayOnRelease(){VinylRelease paused=new VinylRelease(false);paused.stop(true);assertTrue(paused.restore());assertFalse(paused.play());}
    @Test public void focusLossWinsEvenAfterReleaseAndLateDetachCannotResurrect(){VinylRelease playing=new VinylRelease(true);playing.stop(true);assertTrue(playing.play());playing.stop(false);assertFalse(playing.restore());playing.stop(true);assertFalse(playing.play());VinylRelease cancelled=new VinylRelease(true);cancelled.stop(false);cancelled.stop(true);assertFalse(cancelled.play());}
    @Test public void noisyRouteRevokesEveryDeckIncludingQueuedRelease(){
        VinylRelease[] decks={new VinylRelease(true),new VinylRelease(false),new VinylRelease(true),new VinylRelease(true)};
        decks[0].stop(true); // ACTION_UP queued resume before headphone unplug.
        for(VinylRelease deck:decks)deck.stop(false); // Same terminal state used by route pauseAll().
        for(VinylRelease deck:decks){assertTrue(deck.stopped());assertFalse(deck.restore());assertFalse(deck.play());deck.stop(true);assertFalse(deck.play());}
    }
}
