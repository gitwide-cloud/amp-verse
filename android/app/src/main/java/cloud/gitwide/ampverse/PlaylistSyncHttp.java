package cloud.gitwide.ampverse;

import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import org.json.*;

/** Direct selected authority only. Credentials never enter URLs, redirects, or playback items. */
final class PlaylistSyncHttp {
    final String base,password,server,library;
    static final class Reply {final int code;final JSONObject body;Reply(int c,JSONObject b){code=c;body=b;}}
    PlaylistSyncHttp(String base,String password,String server,String library){this.base=base;this.password=password;this.server=server;this.library=library;}
    Reply request(String path,JSONObject body)throws Exception {
        if(password.isEmpty())throw new IOException("A private server access code is required");
        HttpURLConnection c=(HttpURLConnection)new URL(base+path).openConnection(Proxy.NO_PROXY);
        c.setInstanceFollowRedirects(false);c.setConnectTimeout(4000);c.setReadTimeout(8000);c.setRequestProperty("x-amp-password",password);
        if(!server.isEmpty())c.setRequestProperty("x-amp-server",server);if(!library.isEmpty())c.setRequestProperty("x-amp-library",library);
        try{if(body!=null){c.setRequestMethod("POST");c.setDoOutput(true);c.setRequestProperty("Content-Type","application/json");byte[] bytes=body.toString().getBytes(StandardCharsets.UTF_8);if(bytes.length>1024*1024)throw new IOException("Playlist edit exceeds server limit");try(OutputStream o=c.getOutputStream()){o.write(bytes);}}
            int code=c.getResponseCode();if(code>=300&&code<400)throw new IOException("Redirect refused; saved server binding was retained");
            InputStream stream=code<400?c.getInputStream():c.getErrorStream();ByteArrayOutputStream out=new ByteArrayOutputStream();
            if(stream!=null)try(InputStream in=stream){byte[] b=new byte[8192];int n;while((n=in.read(b))!=-1){if(out.size()+n>4*1024*1024)throw new IOException("Server response exceeds sync limit");out.write(b,0,n);}}
            return new Reply(code,out.size()==0?new JSONObject():new JSONObject(out.toString(StandardCharsets.UTF_8.name())));
        }finally{c.disconnect();}
    }
    JSONObject snapshot()throws Exception {Reply r=request("/api/sync/v1/playlists",null);if(r.code!=200)throw new IOException(r.code==401||r.code==403?"Playlist access rejected; reconnect with the private access code":"Playlist server unavailable (HTTP "+r.code+")");return r.body;}
}

