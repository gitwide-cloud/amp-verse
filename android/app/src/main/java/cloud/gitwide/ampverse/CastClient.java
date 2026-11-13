package cloud.gitwide.ampverse;

import org.json.JSONObject;
import java.net.*;
import java.io.*;
import java.nio.charset.StandardCharsets;

/** Direct, explicitly selected AMP Cast server. No discovery or shared-session auto-join. */
final class CastClient {
    final String base;
    private final String password;
    private String serverId="",stationId="";
    private boolean protectedAudio;
    CastClient(String address,String password)throws Exception {
        String input=address.trim();
        if(!input.contains("://"))input="http://"+input;
        URI uri=new URI(input);
        if(!("http".equalsIgnoreCase(uri.getScheme())||"https".equalsIgnoreCase(uri.getScheme()))||uri.getHost()==null||uri.getUserInfo()!=null||uri.getQuery()!=null||uri.getFragment()!=null||!(uri.getPath().isEmpty()||uri.getPath().equals("/")))
            throw new IOException("Enter the server address, for example http://192.168.1.50:8787");
        base=new URI(uri.getScheme().toLowerCase(),null,uri.getHost(),uri.getPort(),null,null,null).toString();this.password=password;
    }
    JSONObject connect()throws Exception {
        JSONObject status=get("/api/status");
        String server=status.optString("server_id"),station=status.optString("station_id");
        if(server.isEmpty()||station.isEmpty())throw new IOException("This server needs an AMP Cast update before phone control is available");
        serverId=server;stationId=station;protectedAudio=status.optJSONObject("capabilities")!=null&&status.getJSONObject("capabilities").optInt("reliable_audio")>=1;return status;
    }
    JSONObject status()throws Exception {
        JSONObject status=get("/api/status");
        if(!serverId.equals(status.optString("server_id"))||!stationId.equals(status.optString("station_id")))throw new IOException("Server or station changed. Connect again before controlling it.");
        return status;
    }
    JSONObject playlist()throws Exception{return get("/api/playlist");}
    void command(boolean start)throws Exception {
        if(serverId.isEmpty())throw new IOException("Connect to a station first");
        request(start?"/api/start":"/api/stop",true);
    }
    void playPlaylist(String id)throws Exception { if(id==null||id.isEmpty()||id.length()>512)throw new IOException("Invalid playlist"); request("/api/smart/playlists/"+android.net.Uri.encode(id)+"/play",true); }
    String protection(){String active=ProtectedCastSource.status(serverId,stationId);return protectedAudio?(active.isEmpty()?"Phone recovery buffer ready":active):"Legacy stream · update server for recovery protection";}
    String stream(){return base+"/listen.mp3";}
    String phoneStream(){return protectedAudio?base+"/amp-protected.mp3?server="+android.net.Uri.encode(serverId)+"&station="+android.net.Uri.encode(stationId):stream();}
    void download(int id,java.io.File file,long expected,String checksum)throws Exception {
        download(id,file,expected,checksum,()->false);
    }
    void download(int id,java.io.File file,long expected,String checksum,java.util.function.BooleanSupplier cancelled)throws Exception {
        if(expected<0||expected>2L*1024*1024*1024)throw new IOException("Track exceeds download limit");
        HttpURLConnection connection=(HttpURLConnection)new URL(base+"/api/smart/tracks/"+id+"/audio").openConnection();
        connection.setInstanceFollowRedirects(false);connection.setConnectTimeout(5000);connection.setReadTimeout(20000);
        if(!password.isEmpty())connection.setRequestProperty("x-amp-password",password);
        java.util.concurrent.atomic.AtomicBoolean done=new java.util.concurrent.atomic.AtomicBoolean();
        Thread watcher=new Thread(()->{while(!done.get()){if(cancelled.getAsBoolean()){connection.disconnect();return;}try{Thread.sleep(100);}catch(InterruptedException e){return;}}},"amp-offline-cancel");watcher.setDaemon(true);watcher.start();
        try {
            if(cancelled.getAsBoolean())throw new IOException("Download cancelled");
            if(connection.getResponseCode()!=200)throw new IOException("Song download rejected: "+connection.getResponseCode());
            java.security.MessageDigest digest=java.security.MessageDigest.getInstance("SHA-256");long total=0;
            try(InputStream input=connection.getInputStream();FileOutputStream output=new FileOutputStream(file)) {
                byte[] buffer=new byte[65536];int n;
                while((n=input.read(buffer))!=-1){if(Thread.currentThread().isInterrupted()||cancelled.getAsBoolean())throw new IOException("Download cancelled");total+=n;if(total>expected)throw new IOException("Audio changed during download");output.write(buffer,0,n);digest.update(buffer,0,n);}
                output.getFD().sync();
            }
            StringBuilder hash=new StringBuilder();for(byte b:digest.digest())hash.append(String.format(java.util.Locale.ROOT,"%02x",b&255));
            if(total!=expected||!hash.toString().equalsIgnoreCase(checksum))throw new IOException("Song integrity check failed");
        } finally {done.set(true);watcher.interrupt();connection.disconnect();}
    }
    JSONObject get(String path)throws Exception {return new JSONObject(request(path,false));}
    void post(String path,JSONObject body)throws Exception {request(path,true,body);}
    private String request(String path,boolean command)throws Exception {return request(path,command,null);}
    private String request(String path,boolean command,JSONObject body)throws Exception {
        HttpURLConnection connection=(HttpURLConnection)new URL(base+path).openConnection();
        connection.setInstanceFollowRedirects(false);connection.setConnectTimeout(4000);connection.setReadTimeout(6000);
        try {
            if(!password.isEmpty())connection.setRequestProperty("x-amp-password",password);
            if(command){connection.setRequestMethod("POST");connection.setRequestProperty("x-amp-server",serverId);connection.setRequestProperty("x-amp-station",stationId);if(!password.isEmpty())connection.setRequestProperty("x-amp-password",password);connection.setDoOutput(true);if(body!=null)connection.setRequestProperty("Content-Type","application/json");try(OutputStream output=connection.getOutputStream()){if(body!=null)output.write(body.toString().getBytes(StandardCharsets.UTF_8));}}
            int code=connection.getResponseCode();
            if(code==401||code==403)throw new IOException("Station control rejected. Check the admin password and selected station.");
            if(code<200||code>=300){String message="AMP Cast returned HTTP "+code;InputStream failure=connection.getErrorStream();if(failure!=null)try(InputStream input=failure){byte[] data=new byte[2048];int n=input.read(data);if(n>0)message+=" · "+new String(data,0,n,StandardCharsets.UTF_8);}throw new IOException(message);}
            if(code==204)return "";
            try(InputStream input=connection.getInputStream();ByteArrayOutputStream output=new ByteArrayOutputStream()) {
                byte[] chunk=new byte[4096];int count;
                while((count=input.read(chunk))!=-1){if(output.size()+count>8*1024*1024)throw new IOException("Station response too large");output.write(chunk,0,count);}
                return output.toString(StandardCharsets.UTF_8.name());
            }
        }finally{connection.disconnect();}
    }
}
