package cloud.gitwide.ampverse;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import org.json.JSONObject;
import java.util.concurrent.*;
import java.io.*;
/** One short broadcast search, never a subnet scan or automatic station join. */
final class CastDiscovery {
    static final int PORT=18787;
    static final byte[] QUERY="AMPVERSE_CAST_DISCOVER_V1".getBytes(StandardCharsets.UTF_8);
    static final class Server {
        final String name,url,id;
        final List<String> paths;
        Server(String name,String url,String id){this(name,url,id,Collections.singletonList(url));}
        Server(String name,String url,String id,List<String> paths){this.name=name;this.url=url;this.id=id;this.paths=paths;}
    }
    static Server parse(byte[] data,int size,InetAddress peer)throws Exception {
        JSONObject json=new JSONObject(new String(data,0,size,StandardCharsets.UTF_8));
        int port=json.optInt("port");String id=json.optString("server_id"),station=json.optString("station_id"),name=json.optString("name","AMP Cast");
        if(json.optInt("protocol")!=1||!json.optString("service").equals("amp-cast")||id.isEmpty()||station.isEmpty()||id.length()>256||station.length()>256||port<1||port>65535||name.length()>256)throw new java.io.IOException("Unsupported server reply");
        String primary=new URI("http",null,peer.getHostAddress(),port,null,null,null).toString();
        List<String> paths=new ArrayList<>();paths.add(primary);
        org.json.JSONArray advertised=json.optJSONArray("paths");
        if(advertised!=null)for(int i=0;i<Math.min(8,advertised.length());i++){
            String candidate=advertised.optString(i);
            if(safePath(candidate,port,peer)&&!paths.contains(candidate))paths.add(candidate);
        }
        return new Server(name,primary,id+"/"+station,paths);
    }
    static boolean safePath(String candidate,int port,InetAddress peer){
        try{
            URI uri=new URI(candidate);
            if(!"http".equals(uri.getScheme())||uri.getHost()==null||uri.getPort()!=port||uri.getUserInfo()!=null||uri.getQuery()!=null||uri.getFragment()!=null||!(uri.getPath().isEmpty()||"/".equals(uri.getPath())))return false;
            // Advertisements may contain only IP literals; DNS is never discovery authority.
            String host=uri.getHost();if(!host.matches("[0-9.]+")&&!host.contains(":"))return false;
            InetAddress address=InetAddress.getByName(host);
            return !address.isAnyLocalAddress()&&!address.isMulticastAddress()&&(address.isSiteLocalAddress()||(peer.isLoopbackAddress()&&address.isLoopbackAddress()));
        }catch(Exception error){return false;}
    }
    private static final okhttp3.OkHttpClient PROBE=new okhttp3.OkHttpClient.Builder().proxy(Proxy.NO_PROXY).followRedirects(false).followSslRedirects(false)
        .connectTimeout(700,TimeUnit.MILLISECONDS).readTimeout(700,TimeUnit.MILLISECONDS).callTimeout(1200,TimeUnit.MILLISECONDS).build();
    private static boolean verified(String path,String identity){
        okhttp3.Call call=PROBE.newCall(new okhttp3.Request.Builder().url(path+"/api/status").build());
        try(okhttp3.Response response=call.execute()){
            if(response.code()!=200||response.body()==null)return false;
            try(InputStream input=response.body().byteStream();ByteArrayOutputStream bytes=new ByteArrayOutputStream()){
                byte[] buffer=new byte[2048];int count;
                while((count=input.read(buffer))!=-1){if(Thread.currentThread().isInterrupted()||bytes.size()+count>65536){call.cancel();return false;}bytes.write(buffer,0,count);}
                JSONObject status=new JSONObject(bytes.toString("UTF-8"));return identity.equals(status.optString("server_id")+"/"+status.optString("station_id"));
            }
        }catch(Exception error){return false;}
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
                try{socket.receive(packet);Server server=parse(data,packet.getLength(),packet.getAddress());Server old=found.get(server.id);
                    if(old==null&&found.size()<16)found.put(server.id,server);
                    else if(old!=null){List<String> merged=new ArrayList<>(old.paths);for(String path:server.paths)if(merged.size()<9&&!merged.contains(path))merged.add(path);found.put(server.id,new Server(old.name,old.url,old.id,merged));}}catch(SocketTimeoutException ignored){}catch(org.json.JSONException|IllegalArgumentException|java.io.IOException ignored){}
            }
        }
        List<Server> reachable=new ArrayList<>();
        ExecutorService probes=Executors.newFixedThreadPool(3);
        try{
            long probeDeadline=System.nanoTime()+TimeUnit.SECONDS.toNanos(5);
            for(Server server:found.values()){
                if(System.nanoTime()>=probeDeadline||Thread.currentThread().isInterrupted())break;
                CompletionService<String> completed=new ExecutorCompletionService<>(probes);List<Future<String>> pending=new ArrayList<>();
                for(String path:server.paths)pending.add(completed.submit(()->verified(path,server.id)?path:null));
                try{
                    long until=Math.min(probeDeadline,System.nanoTime()+TimeUnit.SECONDS.toNanos(4));
                    for(int i=0;i<pending.size();i++){
                        long remaining=until-System.nanoTime();if(remaining<=0)break;
                        Future<String> result=completed.poll(remaining,TimeUnit.NANOSECONDS);if(result==null)break;
                        String url=result.get();if(url!=null){reachable.add(new Server(server.name,url,server.id,server.paths));break;}
                    }
                }finally{for(Future<String> probe:pending)probe.cancel(true);}
            }
        }finally{probes.shutdownNow();}
        return reachable;
    }
}
