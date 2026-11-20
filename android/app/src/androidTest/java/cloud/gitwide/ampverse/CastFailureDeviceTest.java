package cloud.gitwide.ampverse;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;
import java.net.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
/** Generated same-device HTTP fixture, not real LAN acceptance. */
@RunWith(AndroidJUnit4.class)
public class CastFailureDeviceTest {
    @Test public void commandRejectionsAreDistinctFromLostHostReachability()throws Exception {
        ServerSocket server=new ServerSocket(0,8,CastTestAddress.privateAddress());
        java.util.concurrent.atomic.AtomicBoolean changed=new java.util.concurrent.atomic.AtomicBoolean();
        Thread fixture=new Thread(()->{
            while(!server.isClosed())try(Socket socket=server.accept()){
                BufferedReader reader=new BufferedReader(new InputStreamReader(socket.getInputStream(),StandardCharsets.US_ASCII));
                String first=reader.readLine(),line;while((line=reader.readLine())!=null&&!line.isEmpty()){}
                String path=first.split(" ")[1];int code=path.startsWith("/reject/")?Integer.parseInt(path.substring(8)):200;
                byte[] body=(code==200?("{\"server_id\":\""+(changed.get()?"different":"fixture")+"\",\"station_id\":\"station\",\"capabilities\":{\"reliable_audio\":1},\"hls_url\":\"/api/audio/hls/master.m3u8\"}"):"fixture command rejected").getBytes(StandardCharsets.UTF_8);
                OutputStream out=socket.getOutputStream();out.write(("HTTP/1.1 "+code+" Fixture\r\nContent-Length: "+body.length+"\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));out.write(body);out.flush();
            }catch(IOException ignored){if(!server.isClosed())throw new RuntimeException(ignored);}
        });fixture.start();
        CastClient client=new CastClient("http://"+server.getInetAddress().getHostAddress()+":"+server.getLocalPort(),"");
        try{
            client.connect();
            assertTrue("Adaptive HLS must survive Cast error-handling fixes",client.phoneStream().contains("/api/audio/hls/master.m3u8?server=fixture&station=station"));
            for(int code:new int[]{409,503}){
                try{client.post("/reject/"+code,null);fail("Expected command rejection");}
                catch(CastClient.HttpException expected){assertEquals(code,expected.code);}
                assertEquals("fixture",client.status().getString("server_id"));
            }
            for(int code:new int[]{401,403}){
                try{client.post("/reject/"+code,null);fail("Expected authorization rejection");}
                catch(CastClient.AuthorizationException expected){assertEquals(code,expected.code);}
                assertEquals("fixture",client.status().getString("server_id"));
            }
            changed.set(true);
            try{client.status();fail("Changed host identity must invalidate verification");}
            catch(CastClient.HostUnavailableException expected){assertTrue(expected.getMessage().contains("changed"));}
        }finally{server.close();fixture.join(2000);}
        try{client.status();fail("Closed fixture should be unreachable");}
        catch(CastClient.HostUnavailableException expected){assertNotNull(expected.getMessage());}
    }
}
