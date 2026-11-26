package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;
public class VinylGestureTest {
    @Test public void forwardAndBackwardQuarterTurnsReadOppositeAudioDirections(){VinylGesture hand=new VinylGesture();hand.down(1,0);assertEquals(.45,hand.move(0,1),1e-9);assertEquals(-.45,hand.move(1,0),1e-9);hand.up();assertEquals(0,hand.move(0,1),0);}
    @Test public void crossingTheAngularSeamDoesNotJumpAnEntireSong(){VinylGesture hand=new VinylGesture();double a=Math.toRadians(179),b=Math.toRadians(-179);hand.down(Math.cos(a),Math.sin(a));assertEquals(.01,hand.move(Math.cos(b),Math.sin(b)),1e-9);assertEquals(-.01,hand.move(Math.cos(a),Math.sin(a)),1e-9);}
    @Test public void centerTouchAndReentryCannotInjectLargeScratchJumps(){VinylGesture hand=new VinylGesture();hand.down(0,0);assertEquals(0,hand.move(-1,0),0);assertEquals(.45,hand.move(0,-1),1e-9);assertEquals(0,hand.move(0,0),0);assertEquals(0,hand.move(1,0),0);assertEquals(.45,hand.move(0,1),1e-9);}
    @Test public void widePlatterUsesSameRadiusAndRejectsBlankRow(){
        VinylGesture square=new VinylGesture(),wide=new VinylGesture();
        square.downPixels(49,0,49);wide.downPixels(49,0,49);
        assertEquals(square.movePixels(0,49,49),wide.movePixels(0,49,49),0);
        assertEquals(.45,wide.movePixels(-49,0,49),.00001);
        assertTrue(VinylGesture.hit(49,0,49,8));assertFalse(VinylGesture.hit(300,0,49,8));
    }
}
