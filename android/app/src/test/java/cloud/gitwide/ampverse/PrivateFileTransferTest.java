package cloud.gitwide.ampverse;
import org.junit.Test;
import org.junit.Rule;
import org.junit.rules.TemporaryFolder;
import java.io.*;
import java.nio.file.Files;
import static org.junit.Assert.*;
public class PrivateFileTransferTest {
 @Rule public TemporaryFolder temp=new TemporaryFolder();
 @Test public void completedDownloadIsFsyncedWithLengthAndHash()throws Exception {File f=new File(temp.getRoot(),"new.audio");PrivateFileTransfer.Result r=PrivateFileTransfer.copy(new ByteArrayInputStream("abc".getBytes(java.nio.charset.StandardCharsets.UTF_8)),f,3,100,()->false,n->{});assertEquals(3,r.bytes);assertEquals("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",r.sha256);assertEquals("abc",new String(Files.readAllBytes(f.toPath()),java.nio.charset.StandardCharsets.UTF_8));}
 @Test public void partialTruncatedOversizeAndCancelledNeverPublishStagedAudio()throws Exception {
  for(int mode=0;mode<4;mode++){File f=new File(temp.getRoot(),"failed"+mode);final int selected=mode;InputStream in=mode==0?new InputStream(){int count;public int read()throws IOException{if(count++==1)throw new IOException("interrupted");return 1;}}:new ByteArrayInputStream(new byte[]{1,2,3});
   try{PrivateFileTransfer.copy(in,f,mode==1?4:-1,mode==2?2:100,()->selected==3,n->{});fail("mode "+mode);}catch(Exception expected){}assertFalse(f.exists());}
 }
 @Test public void repeatedDownloadCannotOverwriteAnExistingAudioFile()throws Exception {File f=temp.newFile("old.audio");Files.write(f.toPath(),"old cues/audio kept".getBytes(java.nio.charset.StandardCharsets.UTF_8));try{PrivateFileTransfer.copy(new ByteArrayInputStream(new byte[]{1}),f,1,10,()->false,n->{});fail();}catch(IOException expected){}assertEquals("old cues/audio kept",new String(Files.readAllBytes(f.toPath()),java.nio.charset.StandardCharsets.UTF_8));}
}
