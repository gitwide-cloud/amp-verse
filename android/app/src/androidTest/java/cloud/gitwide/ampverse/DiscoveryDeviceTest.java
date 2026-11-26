package cloud.gitwide.ampverse;
import java.net.*;
import java.util.*;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
public class DiscoveryDeviceTest {
    @Test public void findsValidReplyAndIgnoresMalformedPackets()throws Exception {
        try(java.net.ServerSocket http=new java.net.ServerSocket(0,16,InetAddress.getByName("127.0.0.2"));DatagramSocket socket=new DatagramSocket(new InetSocketAddress(InetAddress.getByName("127.0.0.1"),CastDiscovery.PORT))) {
            socket.setSoTimeout(4500);http.setSoTimeout(9000);
            Thread web=new Thread(()->{try{while(!http.isClosed())try(java.net.Socket client=http.accept()){
                java.io.BufferedReader input=new java.io.BufferedReader(new java.io.InputStreamReader(client.getInputStream(),java.nio.charset.StandardCharsets.US_ASCII));
                String line;while((line=input.readLine())!=null&&!line.isEmpty()){}
                byte[] body="{\"server_id\":\"discovery-test\",\"station_id\":\"station-test\"}".getBytes(java.nio.charset.StandardCharsets.UTF_8);
                client.getOutputStream().write(("HTTP/1.1 200 OK\r\nContent-Length: "+body.length+"\r\nConnection: close\r\n\r\n").getBytes(java.nio.charset.StandardCharsets.US_ASCII));client.getOutputStream().write(body);
            }}catch(Exception ignored){}});web.start();
            Thread server=new Thread(()->{try{long until=System.currentTimeMillis()+4000;
                while(System.currentTimeMillis()<until){byte[] input=new byte[256];DatagramPacket request=new DatagramPacket(input,input.length);socket.receive(request);
                    if(!Arrays.equals(CastDiscovery.QUERY,Arrays.copyOf(input,request.getLength())))continue;
                    byte[] malformed="{}".getBytes(java.nio.charset.StandardCharsets.UTF_8);socket.send(new DatagramPacket(malformed,malformed.length,request.getAddress(),request.getPort()));
                    byte[] valid=new JSONObject().put("protocol",1).put("service","amp-cast").put("server_id","discovery-test").put("station_id","station-test").put("name","Test radio").put("port",http.getLocalPort()).put("paths",new org.json.JSONArray().put("http://127.0.0.2:"+http.getLocalPort())).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);
                    socket.send(new DatagramPacket(valid,valid.length,request.getAddress(),request.getPort()));
                }
            }catch(Exception ignored){}});server.start();
            try {
                java.util.List<CastDiscovery.Server> servers=CastDiscovery.find();
                CastDiscovery.Server found=servers.stream().filter(s->s.id.equals("discovery-test/station-test")).findFirst().orElseThrow(()->new AssertionError("No valid discovery reply"));
                assertEquals("http://127.0.0.2:"+http.getLocalPort(),found.url);assertEquals("Test radio",found.name);
                assertEquals(1,servers.stream().filter(s->s.id.equals(found.id)).count());
            }finally{socket.close();http.close();server.join(1500);web.join(1500);}
        }
    }
}
