package cloud.gitwide.ampverse;
import android.content.Context;
import android.net.Uri;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import java.io.*;
import java.util.concurrent.atomic.AtomicBoolean;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;
import static org.junit.Assume.assumeTrue;

/** Generated cache-only fixture: no microphone, owner library, service, or audible playback. */
@RunWith(AndroidJUnit4.class)
public class VinylDecoderDeviceTest {
    private static void le(DataOutputStream out,int value,int bytes)throws IOException{for(int i=0;i<bytes;i++)out.writeByte(value>>>(i*8));}
    @Test public void fullDecoderBoundsThirtySecondFixtureAndProducesRealReversePcm()throws Exception{
        assumeTrue("Real scratch decoder is Full-only",VinylDecoder.available());
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();File source=File.createTempFile("qa-vinyl-",".wav",context.getCacheDir());
        try{
            try(DataOutputStream out=new DataOutputStream(new BufferedOutputStream(new FileOutputStream(source)))){
                int size=48000*30*4;out.writeBytes("RIFF");le(out,size+36,4);out.writeBytes("WAVEfmt ");le(out,16,4);le(out,1,2);le(out,2,2);le(out,48000,4);le(out,48000*4,4);le(out,4,2);le(out,16,2);out.writeBytes("data");le(out,size,4);
                for(int frame=0;frame<48000*30;frame++){int sample=(int)(Math.sin(frame*2*Math.PI*440/48000)*12000);le(out,sample,2);le(out,sample,2);}
            }
            long original=source.length();VinylPcm pcm=VinylDecoder.full().decode(context,Uri.fromFile(source),15000,new AtomicBoolean());
            assertEquals(11000,pcm.startMs);assertTrue(pcm.frames<=48000*8);assertTrue(pcm.frames>=48000*7);assertEquals(original,source.length());
            pcm.hold(1_000_000_000L);pcm.drag(-4800,1_100_000_000L);short[] audio=new short[9600];pcm.render(audio,4800);
            long energy=0;for(short sample:audio)energy+=Math.abs((int)sample);assertTrue(energy>100000);assertTrue(pcm.positionMs()<15000);
            AtomicBoolean cancel=new AtomicBoolean(true);try{VinylDecoder.full().decode(context,Uri.fromFile(source),15000,cancel);fail("Cancelled preparation accepted");}catch(IOException expected){}
            VinylPcm next=VinylDecoder.full().decode(context,Uri.fromFile(source),15000,new AtomicBoolean());assertTrue(next.frames>0);
        }finally{assertTrue(source.delete()||!source.exists());}
    }
}
