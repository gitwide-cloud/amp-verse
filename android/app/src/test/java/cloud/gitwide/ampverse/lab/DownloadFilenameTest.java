package cloud.gitwide.ampverse.lab;
import org.junit.Test;
import java.nio.charset.StandardCharsets;
import static org.junit.Assert.*;
public final class DownloadFilenameTest {
 @Test public void namesRemainPortableAndLeaveRoomForUniqueSuffix(){assertEquals("_CON",UrlDownload.safeTitle("CON"));assertEquals("YouTube audio",UrlDownload.safeTitle("... "));String title=UrlDownload.safeTitle("😀".repeat(150)+"track");assertTrue(title.getBytes(StandardCharsets.UTF_8).length<=160);assertFalse(title.endsWith("."));assertEquals(-1,UrlDownload.safeTitle("bad:/\\\u0001*?\"name").indexOf('/'));assertFalse(title.endsWith("\ud83d"));}
 @Test public void collidingDownloadIdentityCannotOverwriteACompletedSource()throws Exception{
  java.nio.file.Path dir=java.nio.file.Files.createTempDirectory("amp-publish-test-");java.io.File one=dir.resolve("first.pending").toFile(),two=dir.resolve("second.pending").toFile();java.util.UUID identity=java.util.UUID.randomUUID();
  try{java.nio.file.Files.write(one.toPath(),new byte[]{1,2,3});java.nio.file.Files.write(two.toPath(),new byte[]{4,5,6});java.io.File completed=UrlDownload.publish(one,dir.toFile(),"same title",identity);try{UrlDownload.publish(two,dir.toFile(),"same title",identity);fail("A completed source must never be replaced");}catch(java.nio.file.FileAlreadyExistsException expected){}assertArrayEquals(new byte[]{1,2,3},java.nio.file.Files.readAllBytes(completed.toPath()));assertArrayEquals(new byte[]{4,5,6},java.nio.file.Files.readAllBytes(two.toPath()));}
  finally{try(java.util.stream.Stream<java.nio.file.Path> paths=java.nio.file.Files.list(dir)){paths.forEach(path->{try{java.nio.file.Files.delete(path);}catch(java.io.IOException ignored){}});}java.nio.file.Files.delete(dir);}
 }
}
