package cloud.gitwide.ampverse;

import android.content.Context;
import android.content.SharedPreferences;
import androidx.media3.common.C;
import androidx.media3.common.audio.AudioProcessor;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
@androidx.annotation.OptIn(markerClass = androidx.media3.common.util.UnstableApi.class)
public class AudioFeatureDeviceTest {
    private double render(SharedPreferences settings)throws Exception{
        AudioDsp.load(settings);AudioDsp dsp=new AudioDsp();dsp.configure(new AudioProcessor.AudioFormat(44100,1,C.ENCODING_PCM_16BIT));dsp.flush();
        ByteBuffer input=ByteBuffer.allocateDirect(8192).order(ByteOrder.LITTLE_ENDIAN);
        for(int i=0;i<4096;i++)input.putShort((short)(Math.sin(2*Math.PI*1000*i/44100)*8000));input.flip();dsp.queueInput(input);
        ByteBuffer output=dsp.getOutput().order(ByteOrder.LITTLE_ENDIAN);double sum=0;int count=0;
        while(output.remaining()>=2){double value=output.getShort();if(count++>512)sum+=value*value;}
        assertEquals(4096,count);return Math.sqrt(sum/(count-513));
    }
    @Test public void equalizerChangesActualPcmAndBypassPreservesIt()throws Exception{
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();SharedPreferences qa=context.getSharedPreferences("qa-dsp",Context.MODE_PRIVATE);
        try{
            qa.edit().clear().putBoolean("eq-enabled",false).commit();double bypass=render(qa);assertTrue(bypass>5500&&bypass<5800);
            qa.edit().putBoolean("eq-enabled",true).putInt("eq-band-2",-12).commit();double cut=render(qa);assertTrue(cut<bypass*.4);
            qa.edit().putInt("eq-band-2",0).commit();double flat=render(qa);assertEquals(bypass,flat,2);
            double[] tone=new double[1024];for(int i=0;i<tone.length;i++)tone[i]=.5*Math.sin(2*Math.PI*1000*i/44100);
            AudioDsp.spectrumTime=0;AudioDsp.analyze(tone,44100);float[] levels=AudioDsp.spectrum;int peak=0;for(int i=1;i<levels.length;i++)if(levels[i]>levels[peak])peak=i;assertTrue(peak>=10&&peak<=13);assertTrue(levels[peak]>.3);
            AudioDsp.spectrumTime=0;AudioDsp.analyze(new double[1024],44100);for(float value:AudioDsp.spectrum)assertEquals(0,value,0);
        }finally{qa.edit().clear().commit();AudioDsp.load(context.getSharedPreferences("settings",Context.MODE_PRIVATE));}
    }
}
