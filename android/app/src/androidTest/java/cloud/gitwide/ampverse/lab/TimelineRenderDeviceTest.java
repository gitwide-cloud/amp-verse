package cloud.gitwide.ampverse.lab;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import com.arthenica.ffmpegkit.*;
import android.media.MediaMetadataRetriever;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.io.*;
import java.nio.file.Files;
import java.util.*;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public final class TimelineRenderDeviceTest {
 @Test public void overlappingTrimmedLanesRenderContinuouslyWithoutChangingSources()throws Exception{
  File dir=Files.createTempDirectory(InstrumentationRegistry.getInstrumentation().getTargetContext().getCacheDir().toPath(),"timeline-fixture-").toFile();File first=new File(dir,"first.wav"),second=new File(dir,"second.wav"),output=new File(dir,"mix.mp3");
  try{for(int i=0;i<2;i++){FFmpegSession created=FFmpegKit.executeWithArguments(new String[]{"-f","lavfi","-i","sine=frequency="+(i==0?440:880)+":duration=2","-c:a","pcm_s16le",(i==0?first:second).toString()});assertTrue(created.getAllLogsAsString(),ReturnCode.isSuccess(created.getReturnCode()));}
   byte[] originalA=Files.readAllBytes(first.toPath()),originalB=Files.readAllBytes(second.toPath());MobileTimeline.Clip a=new MobileTimeline.Clip(),b=new MobileTimeline.Clip();a.file=first;a.title="A";a.duration=2;a.in=.25;a.out=1.25;a.gain=-6;a.fadeIn=.1;a.fadeOut=.1;b.file=second;b.title="B";b.duration=2;b.in=.5;b.out=1.5;b.position=.5;b.lane=1;b.gain=-3;
   FFmpegSession mixed=FFmpegKit.executeWithArguments(MobileTimeline.arguments(Arrays.asList(a,b),output));assertTrue(mixed.getAllLogsAsString(),ReturnCode.isSuccess(mixed.getReturnCode()));
   MediaMetadataRetriever verify=new MediaMetadataRetriever();try{verify.setDataSource(output.toString());long duration=Long.parseLong(verify.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION));assertTrue("Expected a 1.5-second overlap mix, got "+duration,duration>=1450&&duration<=1650);}finally{verify.release();}
   assertArrayEquals(originalA,Files.readAllBytes(first.toPath()));assertArrayEquals(originalB,Files.readAllBytes(second.toPath()));
   b.position=3600;String[] longArrangement=MobileTimeline.arguments(Arrays.asList(a,b),new File(dir,"long.mp3"));assertFalse("Long offsets must not allocate an adelay ring buffer",String.join(" ",longArrangement).contains("adelay"));
   a.out=3;try{MobileTimeline.arguments(Collections.singletonList(a),output);fail("Invalid trim must be rejected before rendering");}catch(IOException expected){assertTrue(expected.getMessage().contains("trim"));}
  }finally{File[] files=dir.listFiles();if(files!=null)for(File f:files)f.delete();dir.delete();}
 }
}
