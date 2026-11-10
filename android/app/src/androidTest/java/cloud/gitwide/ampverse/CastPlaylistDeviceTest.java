package cloud.gitwide.ampverse;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;
import java.net.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.atomic.AtomicReference;
@RunWith(AndroidJUnit4.class)
public class CastPlaylistDeviceTest {
 @Test public void playlistPlayTargetsConnectedStationWithCredentials()throws Exception {
  ServerSocket server=new ServerSocket(0,4,InetAddress.getByName("127.0.0.1"));AtomicReference<String> command=new AtomicReference<>();
  Thread worker=new Thread(()->{for(int i=0;i<2;i++)try(Socket socket=server.accept()){
   BufferedReader input=new BufferedReader(new InputStreamReader(socket.getInputStream(),StandardCharsets.US_ASCII));StringBuilder request=new StringBuilder();String line;while((line=input.readLine())!=null&&!line.isEmpty())request.append(line).append('\n');
   boolean post=request.toString().startsWith("POST");if(post)command.set(request.toString());
   byte[] body=post?new byte[0]:"{\"server_id\":\"my-server\",\"station_id\":\"my-station\",\"running\":false}".getBytes(StandardCharsets.UTF_8);
   OutputStream output=socket.getOutputStream();output.write(((post?"HTTP/1.1 204 No Content":"HTTP/1.1 200 OK")+"\r\nContent-Length: "+body.length+"\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));output.write(body);output.flush();
  }catch(Exception error){command.set(error.toString());}});worker.start();
  try {CastClient client=new CastClient("http://127.0.0.1:"+server.getLocalPort(),"secret");client.connect();client.playPlaylist("normal_Friday #1");worker.join(2000);String request=command.get();assertNotNull(request);assertTrue(request.contains("POST /api/smart/playlists/normal_Friday%20%231/play "));assertTrue(request.toLowerCase(java.util.Locale.ROOT).contains("x-amp-server: my-server"));assertTrue(request.toLowerCase(java.util.Locale.ROOT).contains("x-amp-station: my-station"));assertTrue(request.toLowerCase(java.util.Locale.ROOT).contains("x-amp-password: secret"));}
  finally{server.close();worker.join(2000);}
 }
}
