package cloud.gitwide.ampverse.lab;
import com.arthenica.ffmpegkit.*;
import java.io.*;
import java.util.regex.*;

/** Editor waveform decoding only; no BPM, key, stem separation or analysis models. */
final class WaveReader {
    record Data(float[] peaks,double peakDb){}
    static Data read(File source,double seconds,File cache)throws IOException{
        File pcm=File.createTempFile("waveform-",".pcm",cache);
        try{
            FFmpegSession session=FFmpegKit.executeWithArguments(new String[]{"-y","-hide_banner","-threads","1","-i",source.toString(),"-filter_complex_threads","1","-filter_complex","[0:a]asplit=2[wave][stats];[stats]astats=metadata=0:reset=0[meter]","-map","[wave]","-ac","1","-ar","2000","-f","f32le",pcm.toString(),"-map","[meter]","-f","null","-"});
            if(!ReturnCode.isSuccess(session.getReturnCode()))return new Data(new float[0],Double.NaN);
            float[] peaks=new float[(int)Math.min(400000,Math.max(1200,Math.ceil(seconds*2000)))];float peak=0;long index=0;
            try(DataInputStream in=new DataInputStream(new BufferedInputStream(new FileInputStream(pcm)))){
                while(in.available()>=4){float value=Math.abs(Float.intBitsToFloat(Integer.reverseBytes(in.readInt())));int bin=(int)(index++/Math.max(1,seconds*2000)*peaks.length);if(bin<peaks.length)peaks[bin]=Math.max(peaks[bin],value);peak=Math.max(peak,value);}
            }
            double db=peak>0?20*Math.log10(peak):Double.NEGATIVE_INFINITY;Matcher meter=Pattern.compile("Peak level dB:\\s+(-?[0-9.]+|-inf)").matcher(session.getAllLogsAsString());while(meter.find())db=meter.group(1).equals("-inf")?Double.NEGATIVE_INFINITY:Double.parseDouble(meter.group(1));return new Data(peaks,db);
        }finally{pcm.delete();}
    }
    static float[] compact(float[] values){if(values.length<=512)return values.clone();float[] peaks=new float[512];for(int i=0;i<values.length;i++){int bin=(int)((long)i*512/values.length);peaks[bin]=Math.max(peaks[bin],values[i]);}return peaks;}
}
