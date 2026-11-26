package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;

public class VinylPcmTest {
    private short[] ramp(){short[] pcm=new short[12000];for(int frame=0;frame<6000;frame++){pcm[frame*2]=(short)frame;pcm[frame*2+1]=(short)-frame;}return pcm;}
    @Test public void holdIsSilentForwardAndReverseUseActualSourceSamples(){
        short[] source=ramp(),original=source.clone();VinylPcm voice=new VinylPcm(source,48000,0,10);voice.hold(1_000_000);
        short[] held=new short[960];voice.render(held,480);for(short sample:held)assertEquals(0,sample);assertEquals(10,voice.positionMs());
        voice.drag(480,11_000_000);short[] forward=new short[960];voice.render(forward,480);
        assertEquals(source[480*2],forward[0]);assertEquals(source[959*2],forward[958]);assertEquals(-forward[958],forward[959]);assertEquals(20,voice.positionMs());
        voice.drag(-480,21_000_000);short[] reverse=new short[960];voice.render(reverse,480);
        assertEquals(source[960*2],reverse[0]);assertEquals(source[481*2],reverse[958]);assertTrue(reverse[0]>reverse[958]);assertEquals(10,voice.positionMs());
        voice.render(held,480);for(short sample:held)assertEquals(0,sample);assertArrayEquals(original,source);
    }
    @Test public void twiceTheHandSpeedActuallyDoublesPcmReadRate(){
        VinylPcm normal=new VinylPcm(ramp(),48000,0,10),fast=new VinylPcm(ramp(),48000,0,10);normal.hold(1_000_000);fast.hold(1_000_000);
        normal.drag(480,11_000_000);fast.drag(960,11_000_000);short[] a=new short[480],b=new short[480];normal.render(a,240);fast.render(b,240);
        assertEquals(481,a[2]);assertEquals(482,b[2]);assertEquals(15,normal.positionMs());assertEquals(20,fast.positionMs());
    }
    @Test public void releaseUsesPlayedFramesNotQueuedRenderCursorAndWindowIsBounded(){
        VinylPcm voice=new VinylPcm(ramp(),48000,0,10);voice.hold(1_000_000);voice.drag(960,21_000_000);voice.render(new short[1920],960);
        assertEquals(30,voice.positionMs());assertEquals(15,voice.heardPositionMs(240));assertEquals(10,voice.heardPositionMs(0));
        voice.drag(Double.NaN,31_000_000);assertEquals(30,voice.positionMs());
        voice.drag(-999999,41_000_000);voice.render(new short[12000],6000);assertEquals(0,voice.positionMs());
        assertTrue(new VinylPcm(ramp(),48000,0,0).covers(0,1000));
        try{new VinylPcm(new short[48000*2*8+2],48000,0,0);fail("Whole-song buffers must be rejected");}catch(IllegalArgumentException expected){}
    }
    @Test public void fourDeckVoicesNeverShareGesturePosition(){
        VinylPcm[] voices=new VinylPcm[4];for(int deck=0;deck<4;deck++){voices[deck]=new VinylPcm(ramp(),48000,0,10+deck*10);voices[deck].hold(1_000_000);}
        voices[2].drag(480,11_000_000);for(VinylPcm voice:voices)voice.render(new short[960],480);
        assertEquals(10,voices[0].positionMs());assertEquals(20,voices[1].positionMs());assertEquals(40,voices[2].positionMs());assertEquals(40,voices[3].positionMs());
    }
}
