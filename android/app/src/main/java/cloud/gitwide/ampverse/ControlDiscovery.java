package cloud.gitwide.ampverse;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import org.json.JSONObject;
/** One short broadcast search, never a subnet scan or automatic station join. */
final class ControlDiscovery {
    static final int PORT=18790;
    static final byte[] QUERY="AMPVERSE_CONTROL_DISCOVER_V1".getBytes(StandardCharsets.UTF_8);
    static final class Server {
        final String name,url,id;
        Server(String name,String url,String id){this.name=name;this.url=url;this.id=id;}
    }
    static Server parse(byte[] data,int size,InetAddress peer)throws Exception {
        JSONObject json=new JSONObject(new String(data,0,size,StandardCharsets.UTF_8));
        int port=json.optInt("port");String id=json.optString("session"),session=json.optString("session"),name=json.optString("name","AMP Verse");
        if(json.optInt("protocol")!=1||!json.optString("service").equals("amp-control")||id.isEmpty()||session.isEmpty()||port<1||port>65535||name.length()>256)throw new java.io.IOException("Unsupported server reply");
        return new Server(name,"http://"+peer.getHostAddress()+":"+port,id);
    }
    static List<Server> find()throws Exception {
        Map<String,Server> found=new LinkedHashMap<>();Set<InetAddress> destinations=new HashSet<>();
        destinations.add(InetAddress.getByName("255.255.255.255"));
        destinations.add(InetAddress.getByName("127.0.0.1"));
        for(NetworkInterface network:Collections.list(NetworkInterface.getNetworkInterfaces())) {
            if(!network.isUp()||network.isLoopback())continue;
            for(InterfaceAddress address:network.getInterfaceAddresses())if(address.getBroadcast()!=null)destinations.add(address.getBroadcast());
        }
        try(DatagramSocket socket=new DatagramSocket()) {
            socket.setBroadcast(true);socket.setSoTimeout(200);long until=System.currentTimeMillis()+5000,next=0;
            while(System.currentTimeMillis()<until&&!Thread.currentThread().isInterrupted()) {
                if(System.currentTimeMillis()>=next){for(InetAddress target:destinations){try{socket.send(new DatagramPacket(QUERY,QUERY.length,target,PORT));}catch(java.io.IOException ignored){}}next=System.currentTimeMillis()+1000;}
                byte[] data=new byte[2048];DatagramPacket packet=new DatagramPacket(data,data.length);
                try{socket.receive(packet);Server server=parse(data,packet.getLength(),packet.getAddress());found.put(server.id,server);}catch(SocketTimeoutException ignored){}catch(org.json.JSONException|IllegalArgumentException|java.io.IOException ignored){}
            }
        }
        return new ArrayList<>(found.values());
    }
}
