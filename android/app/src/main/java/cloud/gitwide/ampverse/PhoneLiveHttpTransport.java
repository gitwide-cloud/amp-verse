package cloud.gitwide.ampverse;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URI;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;

/** Direct selected-host transport. Credentials and live tokens never enter URLs. */
final class PhoneLiveHttpTransport implements PhoneLiveSession.Transport {
    private final URI base;
    private final String password;
    private final PhoneLiveSession.Identity selected;
    private volatile HttpURLConnection current;
    private final Object publication=new Object();
    private volatile boolean aborted;
    private boolean currentCleanup;
    interface Connections { HttpURLConnection open(URL url) throws IOException; }
    private final Connections connections;

    PhoneLiveHttpTransport(String address,String password,PhoneLiveSession.Identity selected) throws IOException {
        this(address,password,selected,url -> (HttpURLConnection)url.openConnection());
    }
    PhoneLiveHttpTransport(String address,String password,PhoneLiveSession.Identity selected,Connections connections) throws IOException {
        this.connections=connections;
        try {
            URI uri=new URI(address);
            if(!("http".equalsIgnoreCase(uri.getScheme())||"https".equalsIgnoreCase(uri.getScheme()))
                    ||uri.getHost()==null||uri.getUserInfo()!=null||uri.getQuery()!=null||uri.getFragment()!=null
                    ||!(uri.getPath().isEmpty()||uri.getPath().equals("/")))throw new IOException("Invalid selected host");
            base=new URI(uri.getScheme(),null,uri.getHost(),uri.getPort(),"/",null,null);
        }catch(java.net.URISyntaxException error){throw new IOException("Invalid selected host",error);}
        if(selected==null||password==null||password.contains("\r")||password.contains("\n"))throw new IOException("Selected host credentials unavailable");
        this.password=password;this.selected=selected;
    }

    @Override public PhoneLiveSession.Identity verify() throws IOException {
        Response response=request("/api/status",false,null,null,null);
        try {
            JSONObject data=new JSONObject(response.body);
            return new PhoneLiveSession.Identity(data.getString("server_id"),data.getString("station_id"),data.getString("library_id"));
        }catch(Exception error){throw new PhoneLiveSession.Rejected("Host did not provide a usable identity");}
    }

    @Override public PhoneLiveSession.Started start(String previous) throws IOException {
        Response response=request("/api/live/start",true,null,previous,null);
        try {
            JSONObject data=new JSONObject(response.body);
            return new PhoneLiveSession.Started(data.getString("session"),data.getLong("next_sequence"),
                    data.optBoolean("upload_sequence"),data.optBoolean("guarded_resume"));
        }catch(Exception error){throw new PhoneLiveSession.Rejected("Host did not provide reliable live ownership");}
    }

    @Override public long upload(String token,long sequence,byte[] bytes) throws IOException {
        Response response=request("/api/live/audio",true,bytes,null,new Owner(token,sequence));
        try {return Long.parseLong(response.nextSequence);}
        catch(Exception error){throw new PhoneLiveSession.Rejected("Host omitted the live upload acknowledgement");}
    }

    @Override public void stop(String token) throws IOException {
        request("/api/live/stop",true,null,null,new Owner(token,-1));
    }
    @Override public void abort(){
        HttpURLConnection connection;
        synchronized(publication){aborted=true;connection=currentCleanup?null:current;}
        if(connection!=null)connection.disconnect();
    }
    private void checkActive(boolean cleanup)throws PhoneLiveSession.Rejected {
        if(aborted&&!cleanup)throw new PhoneLiveSession.Rejected("Phone broadcast cancelled");
    }

    private Response request(String route,boolean post,byte[] bytes,String previous,Owner owner) throws IOException {
        boolean cleanup=route.equals("/api/live/stop");
        checkActive(cleanup);
        HttpURLConnection connection=connections.open(new URL(base.resolve(route).toString()));
        boolean rejected;
        synchronized(publication){
            rejected=aborted&&!cleanup;
            if(!rejected){current=connection;currentCleanup=cleanup;}
        }
        if(rejected){connection.disconnect();throw new PhoneLiveSession.Rejected("Phone broadcast cancelled");}
        try {
        connection.setInstanceFollowRedirects(false);connection.setConnectTimeout(1500);connection.setReadTimeout(2000);
        connection.setRequestProperty("x-amp-server",selected.server);connection.setRequestProperty("x-amp-station",selected.station);connection.setRequestProperty("x-amp-library",selected.library);
        if(!password.isEmpty())connection.setRequestProperty("x-amp-password",password);
        if(previous!=null)connection.setRequestProperty("x-amp-previous-session",previous);
        if(owner!=null){connection.setRequestProperty("x-amp-live-session",owner.token);if(owner.sequence>=0)connection.setRequestProperty("x-amp-live-sequence",Long.toString(owner.sequence));}
            checkActive(cleanup);
            if(post){connection.setRequestMethod("POST");connection.setDoOutput(true);connection.setFixedLengthStreamingMode(bytes==null?0:bytes.length);connection.setRequestProperty("Content-Type","application/octet-stream");try(OutputStream out=connection.getOutputStream()){if(bytes!=null)out.write(bytes);}}
            int code=connection.getResponseCode();
            if(code<200||code>=300){
                if(code>=400&&code<500)throw new PhoneLiveSession.Rejected(code==409?"Live owner or upload sequence changed":"Host rejected live access; reconnect explicitly");
                if(code>=300&&code<400)throw new PhoneLiveSession.Rejected("Live host redirected; select it explicitly");
                throw new IOException("Live host temporarily unavailable");
            }
            String next=connection.getHeaderField("x-amp-live-next-sequence");
            if(code==204)return new Response("",next);
            try(InputStream input=connection.getInputStream();ByteArrayOutputStream out=new ByteArrayOutputStream()){
                byte[] buffer=new byte[4096];int n;
                while((n=input.read(buffer))!=-1){if(out.size()+n>65536)throw new PhoneLiveSession.Rejected("Live host response exceeds limit");out.write(buffer,0,n);}
                return new Response(new String(out.toByteArray(),StandardCharsets.UTF_8),next);
            }
        }finally{
            synchronized(publication){if(current==connection){current=null;currentCleanup=false;}}
            connection.disconnect();
        }
    }
    private static final class Owner {
        final String token;final long sequence;
        Owner(String token,long sequence){this.token=token;this.sequence=sequence;}
    }
    private static final class Response {
        final String body,nextSequence;
        Response(String body,String nextSequence){this.body=body;this.nextSequence=nextSequence;}
    }
}
