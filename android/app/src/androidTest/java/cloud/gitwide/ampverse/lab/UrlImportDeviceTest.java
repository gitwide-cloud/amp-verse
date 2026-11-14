package cloud.gitwide.ampverse.lab;
import android.content.Context;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import com.yausername.youtubedl_android.YoutubeDL;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.io.*;
import java.lang.reflect.*;
import java.util.*;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public final class UrlImportDeviceTest {
 @Test public void cancellationKillsTheNativeDownloadGroupPromptly()throws Exception{
  Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();YoutubeDL.INSTANCE.init(c);
  java.net.ServerSocket server=new java.net.ServerSocket(0,1,java.net.InetAddress.getByName("127.0.0.1"));java.util.concurrent.CountDownLatch connected=new java.util.concurrent.CountDownLatch(1),release=new java.util.concurrent.CountDownLatch(1);
  Thread host=new Thread(()->{try(java.net.Socket socket=server.accept()){connected.countDown();release.await(15,java.util.concurrent.TimeUnit.SECONDS);}catch(Exception ignored){}});host.start();
  File work=java.nio.file.Files.createTempDirectory(c.getCacheDir().toPath(),"url-cancel-test-").toFile();
  UrlDownload job=new UrlDownload(c,new UrlDownload.Listener(){public void progress(String s){}public void ready(File f){}public void failed(String s){}});
  java.util.concurrent.atomic.AtomicReference<Throwable> result=new java.util.concurrent.atomic.AtomicReference<>();
  Thread worker=new Thread(()->{try{Method method=UrlDownload.class.getDeclaredMethod("execute",File.class,List.class,boolean.class);method.setAccessible(true);method.invoke(job,work,Arrays.asList("--simulate","--","http://127.0.0.1:"+server.getLocalPort()+"/slow"),false);}catch(Throwable error){result.set(error);}});
  try{worker.start();assertTrue("Native downloader must connect to the local fixture",connected.await(10,java.util.concurrent.TimeUnit.SECONDS));job.cancel();worker.join(3000);assertFalse("Cancel must not wait for the network timeout",worker.isAlive());assertNotNull(result.get());}
  finally{job.cancel();release.countDown();server.close();host.join(1000);File[] files=work.listFiles();if(files!=null)for(File file:files)file.delete();work.delete();}
 }
 @Test public void localMp3ConversionPreservesSourceAndProducesPlayableAudio()throws Exception{
  Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();File source=File.createTempFile("url-source-",".wav",c.getCacheDir()),output=File.createTempFile("url-result-",".mp3",c.getCacheDir());
  try{com.arthenica.ffmpegkit.FFmpegSession generated=com.arthenica.ffmpegkit.FFmpegKit.executeWithArguments(new String[]{"-y","-f","lavfi","-i","sine=frequency=440:duration=2","-c:a","pcm_s16le",source.toString()});assertTrue(com.arthenica.ffmpegkit.ReturnCode.isSuccess(generated.getReturnCode()));byte[] before=java.nio.file.Files.readAllBytes(source.toPath());
   com.arthenica.ffmpegkit.FFmpegSession converted=com.arthenica.ffmpegkit.FFmpegKit.executeWithArguments(new String[]{"-y","-i",source.toString(),"-vn","-c:a","libmp3lame","-b:a","256k",output.toString()});assertTrue(converted.getAllLogsAsString(),com.arthenica.ffmpegkit.ReturnCode.isSuccess(converted.getReturnCode()));assertArrayEquals(before,java.nio.file.Files.readAllBytes(source.toPath()));android.media.MediaMetadataRetriever verify=new android.media.MediaMetadataRetriever();try{verify.setDataSource(output.toString());assertTrue(Long.parseLong(verify.extractMetadata(android.media.MediaMetadataRetriever.METADATA_KEY_DURATION))>=1900);}finally{verify.release();}
  }finally{source.delete();output.delete();}
 }
 @Test public void packagedPythonQuickJsAndExtractorExecuteWithoutDesktopBinaries()throws Exception{
  Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();YoutubeDL.INSTANCE.init(c);
  File bin=new File(c.getApplicationInfo().nativeLibraryDir);assertTrue(new File(bin,"libpython.so").isFile());assertTrue(new File(bin,"libqjs.so").isFile());
  UrlDownload job=new UrlDownload(c,new UrlDownload.Listener(){public void progress(String s){}public void ready(File f){fail("Version check cannot create a source");}public void failed(String s){fail(s);}});
  File work=java.nio.file.Files.createTempDirectory(c.getCacheDir().toPath(),"url-tools-test-").toFile();
  try{Method method=UrlDownload.class.getDeclaredMethod("execute",File.class,List.class,boolean.class);method.setAccessible(true);String version=(String)method.invoke(job,work,Arrays.asList("--version"),true);assertTrue(version,version.matches("(?s).*20[0-9]{2}\\.[0-9]{2}\\.[0-9]{2}.*"));}
  finally{File[] files=work.listFiles();if(files!=null)for(File file:files)file.delete();work.delete();}
 }
}
