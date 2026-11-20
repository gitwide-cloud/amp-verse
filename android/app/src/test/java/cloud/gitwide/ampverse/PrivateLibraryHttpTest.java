package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;
import okhttp3.*;
import java.io.IOException;
import java.util.concurrent.TimeUnit;
public class PrivateLibraryHttpTest {
 @Test public void fileReadUsesOnlyExactPrivateResourceHeadersAndNoStationCommand()throws Exception {
  try(CastLibraryApiTest.Fixture f=new CastLibraryApiTest.Fixture("audio")){
   String base="http://127.0.0.1:"+f.server.getLocalPort(),target=base+"/api/player/audio?library_id=library-A&path=Album%2Fsong.mp3";
   OkHttpClient http=PrivateLibraryHttp.create(base,"private-fixture-code","server-A","library-A",target);
   try(Response response=http.newCall(new Request.Builder().url(target).header("Range","bytes=1-2").build()).execute()){assertEquals("audio",response.body().string());}
   f.done.get(5,TimeUnit.SECONDS);assertEquals(1,f.requests.size());String request=f.requests.get(0);assertTrue(request.startsWith("GET /api/player/audio?"));assertTrue(request.contains("x-amp-password: private-fixture-code"));assertTrue(request.contains("x-amp-server: server-a"));assertTrue(request.contains("x-amp-library: library-a"));assertTrue(request.contains("range: bytes=1-2"));assertFalse(request.contains("/api/player/play"));assertFalse(http.followRedirects());assertFalse(http.followSslRedirects());
  }
 }
 @Test public void wrongOriginResourceLibraryOrRedirectTargetIsRejectedBeforeNetwork()throws Exception {
  String base="http://127.0.0.1:1",valid=base+"/api/player/audio?library_id=library-A&path=song.mp3";
  for(String target:new String[]{"http://example.invalid/api/player/audio?library_id=library-A&path=song.mp3",base+"/api/status?library_id=library-A",base+"/api/player/audio?library_id=library-B&path=song.mp3"}){
   try{PrivateLibraryHttp.create(base,"secret","server-A","library-A",target).newCall(new Request.Builder().url(target).build()).execute();fail(target);}catch(IOException expected){assertTrue(expected.getMessage().contains("selected authority"));}
  }
  try{PrivateLibraryHttp.create(base,"secret","server-A","library-A",valid).newCall(new Request.Builder().url(base+"/other").build()).execute();fail();}catch(IOException expected){assertTrue(expected.getMessage().contains("selected authority"));}
 }
 @Test public void openLanFileReadAllowsEmptyCredentialWithExactAuthorityHeaders()throws Exception {
  try(CastLibraryApiTest.Fixture f=new CastLibraryApiTest.Fixture("audio")){
   String base="http://127.0.0.1:"+f.server.getLocalPort(),target=base+"/api/player/audio?library_id=library-A&path=song.mp3";
   try(Response response=PrivateLibraryHttp.create(base,"","server-A","library-A",target).newCall(new Request.Builder().url(target).build()).execute()){assertEquals(200,response.code());}
   f.done.get(5,TimeUnit.SECONDS);String request=f.requests.get(0);assertTrue(request.contains("x-amp-server: server-a"));assertTrue(request.contains("x-amp-library: library-a"));
  }
 }
}
