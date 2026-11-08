package cloud.gitwide.ampverse;

import android.content.SharedPreferences;
import android.os.SystemClock;
import androidx.media3.common.C;
import androidx.media3.common.audio.BaseAudioProcessor;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/** Five-band PCM EQ plus a real post-EQ spectrum tap; no microphone permission. */
@androidx.annotation.OptIn(markerClass = androidx.media3.common.util.UnstableApi.class)
final class AudioDsp extends BaseAudioProcessor {
    static final int[] FREQUENCIES={80,250,1000,4000,12000};
    static volatile float[] spectrum=new float[24];
    static volatile long spectrumTime;
    static volatile boolean visible;
    private static volatile float[] gains=new float[5];
    private static volatile boolean enabled;
    private float[] applied;
    private boolean active;
    private int rate=44100,channels=2,frameChannel,count;
    private double sum;
    private final double[] samples=new double[1024];
    private double[][] coefficients;
    private double[][][] memory;
    static void load(SharedPreferences prefs){float[] next=new float[5];for(int i=0;i<5;i++)next[i]=Math.max(-12,Math.min(12,prefs.getInt("eq-band-"+i,0)));gains=next;enabled=prefs.getBoolean("eq-enabled",false);}
    @Override protected AudioFormat onConfigure(AudioFormat format){rate=format.sampleRate;channels=format.channelCount;applied=null;return format.encoding==C.ENCODING_PCM_16BIT?format:AudioFormat.NOT_SET;}
    private void configure(){
        applied=gains;active=enabled;coefficients=new double[5][5];memory=new double[channels][5][2];
        for(int i=0;i<5;i++){
            double amplitude=Math.pow(10,applied[i]/40.),omega=2*Math.PI*Math.min(FREQUENCIES[i],rate*.45)/rate,alpha=Math.sin(omega)/2,cos=Math.cos(omega),a0=1+alpha/amplitude;
            coefficients[i]=new double[]{(1+alpha*amplitude)/a0,-2*cos/a0,(1-alpha*amplitude)/a0,-2*cos/a0,(1-alpha/amplitude)/a0};
        }
    }
    @Override public void queueInput(ByteBuffer input){
        if(applied!=gains||active!=enabled)configure();
        input.order(ByteOrder.LITTLE_ENDIAN);ByteBuffer output=replaceOutputBuffer(input.remaining()).order(ByteOrder.LITTLE_ENDIAN);
        while(input.remaining()>=2){
            double value=input.getShort()/32768.;
            if(active)for(int band=0;band<5;band++){double[] c=coefficients[band],m=memory[frameChannel][band];double filtered=c[0]*value+m[0];m[0]=c[1]*value-c[3]*filtered+m[1];m[1]=c[2]*value-c[4]*filtered;value=filtered;}
            value=Math.max(-1,Math.min(32767./32768.,value));output.putShort((short)Math.round(value*32768));sum+=value;
            if(++frameChannel==channels){samples[count++]=sum/channels;frameChannel=0;sum=0;if(count==samples.length){if(visible)analyze(samples,rate);count=0;}}
        }
        output.flip();
    }
    static void analyze(double[] mono,int rate){
        if(SystemClock.elapsedRealtime()-spectrumTime<32)return;
        float[] levels=new float[24];
        for(int band=0;band<24;band++){
            double frequency=60*Math.pow(Math.min(16000,rate*.45)/60.,band/23.),coefficient=2*Math.cos(2*Math.PI*frequency/rate),previous=0,older=0;
            for(int i=0;i<mono.length;i++){double value=mono[i]*(.5-.5*Math.cos(2*Math.PI*i/(mono.length-1)))+coefficient*previous-older;older=previous;previous=value;}
            double magnitude=2*Math.sqrt(Math.max(0,previous*previous+older*older-coefficient*previous*older))/mono.length;
            levels[band]=(float)Math.max(0,Math.min(1,(20*Math.log10(magnitude+1e-9)+65)/65));
        }
        spectrum=levels;spectrumTime=SystemClock.elapsedRealtime();
    }
    @Override protected void onFlush(){count=0;frameChannel=0;sum=0;applied=null;spectrum=new float[24];}
}
