package cloud.gitwide.ampverse;
import android.content.Context;
import android.net.Uri;
import com.arthenica.ffmpegkit.*;
import java.io.*;
import java.util.Locale;
import java.util.concurrent.atomic.AtomicBoolean;

/** One native job across controller recreation; decode only eight seconds of local PCM. */
final class FullVinylDecoder implements VinylDecoder {
    private static final VinylDecodeGate GATE=new VinylDecodeGate();
    private static boolean running(FFmpegSession session){return session.getState()==SessionState.CREATED||session.getState()==SessionState.RUNNING;}
    private static void remove(File file){if(file!=null&&!file.delete())file.deleteOnExit();}
    @Override public VinylPcm decode(Context context,Uri uri,long positionMs,AtomicBoolean cancelled)throws Exception {
        String scheme=uri.getScheme();if(!"file".equals(scheme)&&!"content".equals(scheme))throw new IOException("Scratch needs a local or downloaded song");
        VinylDecodeGate.Ticket ticket=GATE.acquire();if(ticket==null)throw new IOException("Previous scratch preparation is still stopping");
        long start=Math.max(0,positionMs-4000);File pcm=null;FFmpegSession session=null;
        try{
            pcm=File.createTempFile("amp-vinyl-",".pcm",context.getCacheDir());final File output=pcm;
            String input="content".equals(scheme)?FFmpegKitConfig.getSafParameterForRead(context,uri):new File(uri.getPath()).getCanonicalPath();
            if(cancelled.get())throw new IOException("Scratch preparation cancelled");
            session=FFmpegKit.executeWithArgumentsAsync(new String[]{"-nostdin","-v","error","-y","-threads","1","-ss",String.format(Locale.ROOT,"%.3f",start/1000.),"-protocol_whitelist","file,pipe,saf","-i",input,"-t","8","-map","0:a:0","-vn","-ac","2","-ar","48000","-c:a","pcm_s16le","-f","s16le",pcm.toString()},completed->{if(GATE.terminal(ticket))remove(output);});
            long deadline=android.os.SystemClock.elapsedRealtime()+10000;
            while(running(session)){
                if(cancelled.get()||android.os.SystemClock.elapsedRealtime()>deadline)throw new IOException("Scratch preparation cancelled or timed out");
                Thread.sleep(20);
            }
            GATE.terminal(ticket);
            if(cancelled.get()||!ReturnCode.isSuccess(session.getReturnCode()))throw new IOException("Scratch could not decode this local song; reload or use an offline copy");
            long length=pcm.length();if(length<4||length>48000*2*2*8||length%4!=0)throw new IOException("Scratch window has no usable PCM");
            byte[] bytes=new byte[(int)length];try(DataInputStream in=new DataInputStream(new FileInputStream(pcm))){in.readFully(bytes);}
            short[] samples=new short[bytes.length/2];for(int i=0;i<samples.length;i++)samples[i]=(short)((bytes[i*2]&255)|(bytes[i*2+1]<<8));
            return new VinylPcm(samples,48000,start,positionMs);
        }finally{
            boolean interrupted=Thread.interrupted();
            if(session==null)GATE.terminal(ticket);
            else if(running(session)){
                // Cancel only our own job. A slow native cancellation retains the gate/temp file.
                FFmpegKit.cancel(session.getSessionId());long deadline=android.os.SystemClock.elapsedRealtime()+500;
                while(running(session)&&android.os.SystemClock.elapsedRealtime()<deadline)try{Thread.sleep(10);}catch(InterruptedException stop){interrupted=true;}
                if(!running(session))GATE.terminal(ticket);
            }else GATE.terminal(ticket);
            if(GATE.release(ticket))remove(pcm);
            if(interrupted)Thread.currentThread().interrupt();
        }
    }
}
