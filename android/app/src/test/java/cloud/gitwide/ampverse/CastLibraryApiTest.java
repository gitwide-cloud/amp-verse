package cloud.gitwide.ampverse;
import org.json.*;
import org.junit.Test;
import static org.junit.Assert.*;
import java.net.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;
/** Real bounded loopback HTTP fixture; production LAN-address validation remains unchanged. */
public class CastLibraryApiTest {
 static final class Fixture implements AutoCloseable {
  final ServerSocket server=new ServerSocket(0,4,InetAddress.getByName("127.0.0.1"));
  final ExecutorService worker=Executors.newSingleThreadExecutor();
  final List<String> requests=Collections.synchronizedList(new ArrayList<>());
  final Future<?> done;
  Fixture(String... replies)throws Exception {done=worker.submit(()->{try{for(String reply:replies){try(Socket socket=server.accept()){socket.setSoTimeout(3000);BufferedReader input=new BufferedReader(new InputStreamReader(socket.getInputStream(),StandardCharsets.UTF_8));String first=input.readLine(),line;int length=0;StringBuilder headers=new StringBuilder();while(!(line=input.readLine()).isEmpty()){headers.append(line.toLowerCase(Locale.ROOT)).append("\n");if(line.toLowerCase(Locale.ROOT).startsWith("content-length:"))length=Integer.parseInt(line.substring(15).trim());}if(length>131072)throw new IOException("Fixture request too large");char[] data=new char[length];int read=0;while(read<length){int n=input.read(data,read,length-read);if(n<0)throw new EOFException();read+=n;}requests.add(first+"\n"+headers+"\n"+new String(data));byte[] bytes=reply.getBytes(StandardCharsets.UTF_8);socket.getOutputStream().write(("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: "+bytes.length+"\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));socket.getOutputStream().write(bytes);}}}catch(Exception e){throw new RuntimeException(e);}});}
  CastClient client()throws Exception {
   CastClient c=new CastClient("http://192.0.2.1","private-fixture-code");
   // Test-only replacement after normal validation; never add a production loopback bypass.
   java.lang.reflect.Field base=CastClient.class.getDeclaredField("base");base.setAccessible(true);base.set(c,"http://"+server.getInetAddress().getHostAddress()+":"+server.getLocalPort());return c;
  }
  public void close()throws Exception {server.close();worker.shutdownNow();}
 }
 static String status(String station)throws Exception {return new JSONObject().put("server_id","server-A").put("station_id",station).put("library_id","library-A").put("capabilities",new JSONObject().put("player_queue",1).put("player_track_refs","library_path_v1")).toString();}
 @Test public void selectedPlayerRequestCarriesIdentityAndStableOrderedPathsWithoutSecretUrls()throws Exception {
  try(Fixture f=new Fixture(status("station-A"),status("station-A"),"{}")){
   CastClient c=f.client();assertTrue(CastLibraryModel.supports(c.connect()));c.status();c.post("/api/player/play",CastLibraryModel.play("library-A",new JSONArray().put("Album/song.mp3").put("Album/song.mp3"),false));f.done.get(5,TimeUnit.SECONDS);
   String request=f.requests.get(2);assertTrue(request.startsWith("POST /api/player/play HTTP/1.1"));assertTrue(request.contains("x-amp-password: private-fixture-code"));assertTrue(request.contains("x-amp-server: server-a"));assertTrue(request.contains("x-amp-station: station-a"));assertFalse(request.split("\n")[0].contains("private-fixture"));
   JSONObject body=new JSONObject(request.substring(request.indexOf("\n\n")+2));assertEquals("library-A",body.getString("library_id"));assertEquals(2,body.getJSONArray("tracks").length());assertEquals(body.getJSONArray("tracks").getString(0),body.getJSONArray("tracks").getString(1));assertFalse(body.getBoolean("server_speakers"));
  }
 }
 @Test public void changedStationStopsBeforeAnyPlayerPublication()throws Exception {
  try(Fixture f=new Fixture(status("station-A"),status("station-B"))){CastClient c=f.client();c.connect();try{c.status();fail();}catch(CastClient.HostUnavailableException expected){assertTrue(expected.getMessage().contains("changed"));}f.done.get(5,TimeUnit.SECONDS);assertEquals(2,f.requests.size());}
 }
 @Test public void openLanSyncSnapshotAndOperationDoNotRequireANewCode()throws Exception {
  try(Fixture f=new Fixture("{\"documents\":[]}", "{\"status\":\"applied\"}")){
   String base="http://127.0.0.1:"+f.server.getLocalPort();PlaylistSyncHttp http=new PlaylistSyncHttp(base,"","server-A","library-A");
   assertEquals(200,http.request("/api/sync/v1/playlists",null).code);
   assertEquals(200,http.request("/api/sync/v1/playlist-operations",new JSONObject().put("operation_id","retained-operation")).code);
   f.done.get(5,TimeUnit.SECONDS);for(String request:f.requests){assertTrue(request.contains("x-amp-server: server-a"));assertTrue(request.contains("x-amp-library: library-a"));assertFalse(request.contains("x-amp-password:"));}
  }
 }
}
