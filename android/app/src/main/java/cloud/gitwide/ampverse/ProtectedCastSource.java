package cloud.gitwide.ampverse;

import android.net.Uri;
import androidx.media3.common.C;
import androidx.media3.datasource.*;
import okhttp3.*;
import org.json.*;
import java.io.*;
import java.net.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.concurrent.*;

/** Exact-sequence replay. Two verified server addresses race for the same encoded chunk. */
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
final class ProtectedCastSource extends BaseDataSource {
    static volatile String connectionStatus="";
    private static volatile String statusServer="",statusStation="",statusOwner="";
    static String status(String server,String station){return server.equals(statusServer)&&station.equals(statusStation)?connectionStatus:"";}
    private void report(String text){statusServer=server;statusStation=station;statusOwner=listener;connectionStatus=text;}
    private static final OkHttpClient HTTP=new OkHttpClient.Builder().proxy(Proxy.NO_PROXY)
        .connectTimeout(1000,TimeUnit.MILLISECONDS).readTimeout(1500,TimeUnit.MILLISECONDS)
        .callTimeout(2200,TimeUnit.MILLISECONDS).followRedirects(false).build();
    private final Set<Call> calls=ConcurrentHashMap.newKeySet();
    private final List<String> paths=new CopyOnWriteArrayList<>();
    private ExecutorService workers;
    private volatile boolean closed=true;
    private Uri uri;
    private String server,station,epoch;
    private final String listener=UUID.randomUUID().toString();
    private long sequence,offset;
    private byte[] pending=new byte[0];
    private int at;
    private boolean started;
    ProtectedCastSource(){super(true);}
    static boolean accepts(Uri uri){return "/amp-protected.mp3".equals(uri.getPath());}
    static final class Factory implements DataSource.Factory {
        private final DataSource.Factory ordinary;
        Factory(android.content.Context context){ordinary=new DefaultDataSource.Factory(context);}
        public DataSource createDataSource(){return new DataSource(){
            private DataSource source;
            private ProtectedCastSource protectedSource;
            private final List<TransferListener> listeners=new ArrayList<>();
            public void addTransferListener(TransferListener listener){listeners.add(listener);if(source!=null)source.addTransferListener(listener);}
            public long open(DataSpec spec)throws IOException {
                if(accepts(spec.uri)){if(protectedSource==null)protectedSource=new ProtectedCastSource();source=protectedSource;}
                else source=ordinary.createDataSource();
                for(TransferListener listener:listeners)source.addTransferListener(listener);
                return source.open(spec);
            }
            public int read(byte[] buffer,int start,int length)throws IOException{return source.read(buffer,start,length);}
            public Uri getUri(){return source==null?null:source.getUri();}
            public Map<String,List<String>> getResponseHeaders(){return source==null?Collections.emptyMap():source.getResponseHeaders();}
            public void close()throws IOException{if(source!=null)source.close();}
        };}
    }
    @Override public long open(DataSpec spec)throws IOException {
        transferInitializing(spec);closed=false;workers=Executors.newFixedThreadPool(3);
        boolean resume=spec.uri.equals(uri)&&spec.position==offset&&spec.position>0;
        uri=spec.uri;
        if(!resume){
            paths.clear();pending=new byte[0];at=0;offset=0;
            server=uri.getQueryParameter("server");station=uri.getQueryParameter("station");
            if(server==null||station==null||server.length()>256||station.length()>256)throw new IOException("Select an AMP Cast station again");
            try {
                String primary=new URI(uri.getScheme(),null,uri.getHost(),uri.getPort(),null,null,null).toString();
                JSONObject manifest=manifest(primary);
                epoch=manifest.getString("epoch");sequence=manifest.getLong("start_sequence");
                if(epoch.length()>128||sequence<0)throw new IOException("Invalid station audio session");
                paths.add(primary);report("Protected stream · 1 server link");
                JSONArray alternatives=manifest.optJSONArray("paths");
                if(alternatives!=null){final String selectedEpoch=epoch;workers.submit(()->verifyPaths(primary,selectedEpoch,alternatives));}
            }catch(IOException error){close();throw error;}
            catch(Exception error){close();throw new IOException("Cannot prepare protected station audio",error);}
        }
        transferStarted(spec);started=true;return C.LENGTH_UNSET;
    }
    private void verifyPaths(String primary,String selectedEpoch,JSONArray alternatives){
        for(int i=0;i<Math.min(8,alternatives.length())&&!closed&&paths.size()<2;i++){
            String candidate=alternatives.optString(i);
            if(candidate.equals(primary)||!safeAlternative(primary,candidate))continue;
            try {
                JSONObject value=manifest(candidate);
                if(!closed&&selectedEpoch.equals(epoch)&&selectedEpoch.equals(value.optString("epoch"))){paths.add(candidate);report("DualPath protected · 2 server links");}
            }catch(Exception ignored){/* A down secondary must never interrupt the primary. */}
        }
    }
    static boolean safeAlternative(String primary,String candidate){
        try {
            URI base=new URI(primary),other=new URI(candidate);
            if(!Objects.equals(base.getScheme(),other.getScheme())||other.getUserInfo()!=null||other.getQuery()!=null||other.getFragment()!=null||other.getHost()==null||!(other.getPath().isEmpty()||other.getPath().equals("/")))return false;
            InetAddress address=InetAddress.getByName(other.getHost());
            return !address.isAnyLocalAddress()&&!address.isMulticastAddress()&&(address.isSiteLocalAddress()||other.getHost().equals(base.getHost()));
        }catch(Exception error){return false;}
    }
    private JSONObject manifest(String base)throws Exception {
        try(Response response=execute(request(base+"/api/audio/manifest"))){
            if(response.code()!=200)throw new IOException("Station is preparing audio (HTTP "+response.code()+")");
            JSONObject value=new JSONObject(new String(body(response,16384),java.nio.charset.StandardCharsets.UTF_8));
            if(value.optInt("protocol")!=1||!server.equals(value.optString("server_id"))||!station.equals(value.optString("station_id")))throw new IOException("Station identity changed; reconnect to the station");
            return value;
        }
    }
    private Request request(String url){return new Request.Builder().url(url).header("x-amp-server",server).header("x-amp-station",station).header("x-amp-listener",listener).build();}
    private Response execute(Request request)throws IOException {
        if(closed)throw new InterruptedIOException("Listening stopped");
        Call call=HTTP.newCall(request);calls.add(call);
        try{return call.execute();}finally{calls.remove(call);}
    }
    private static byte[] body(Response response,int limit)throws IOException {
        if(response.body()==null)throw new IOException("Missing audio response");
        try(InputStream input=response.body().byteStream();ByteArrayOutputStream output=new ByteArrayOutputStream()){
            byte[] bytes=new byte[4096];int count;
            while((count=input.read(bytes))!=-1){if(output.size()+count>limit)throw new IOException("Station response is too large");output.write(bytes,0,count);}
            return output.toByteArray();
        }
    }
    private static final class Reset extends IOException {Reset(String message){super(message);}}
    static void validate(byte[] bytes,long expected,String expectedEpoch,String actualSequence,String actualEpoch,String hash)throws IOException {
        if(!Long.toString(expected).equals(actualSequence)||!expectedEpoch.equals(actualEpoch))throw new IOException("Out-of-order station audio rejected");
        try {
            StringBuilder digest=new StringBuilder();for(byte value:MessageDigest.getInstance("SHA-256").digest(bytes))digest.append(String.format(java.util.Locale.ROOT,"%02x",value&255));
            if(bytes.length==0||!digest.toString().equals(hash))throw new IOException("Station audio checksum failed");
        }catch(java.security.GeneralSecurityException error){throw new IOException(error);}
    }
    private byte[] fetch(String base,long wanted)throws IOException {
        String url=base+"/api/audio/chunks/"+wanted+"?epoch="+Uri.encode(epoch);
        try(Response response=execute(request(url))){
            if(response.code()==204)return null;
            if(response.code()==409||response.code()==410)throw new Reset("Station changed or connection exceeded its recovery window");
            if(response.code()!=200)throw new IOException("Audio link HTTP "+response.code());
            byte[] bytes=body(response,65536);
            validate(bytes,wanted,epoch,response.header("x-amp-sequence"),response.header("x-amp-epoch"),response.header("x-amp-sha256"));
            return bytes;
        }
    }
    private byte[] next()throws IOException {
        long until=android.os.SystemClock.elapsedRealtime()+30000;
        while(!closed){
            CompletionService<byte[]> complete=new ExecutorCompletionService<>(workers);
            List<Future<byte[]>> attempts=new ArrayList<>();
            final long wanted=sequence;
            for(String path:paths)attempts.add(complete.submit(()->fetch(path,wanted)));
            Reset reset=null;
            try {
                for(int i=0;i<attempts.size();i++){
                    Future<byte[]> finished=complete.poll(3,TimeUnit.SECONDS);
                    if(finished==null)break;
                    try {byte[] bytes=finished.get();if(bytes!=null)return bytes;}
                    catch(ExecutionException error){if(error.getCause() instanceof Reset)reset=(Reset)error.getCause();}
                }
            }catch(InterruptedException error){Thread.currentThread().interrupt();throw new InterruptedIOException("Listening interrupted");}
            catch(CancellationException error){throw new InterruptedIOException("Listening stopped");}
            finally{for(Future<byte[]> attempt:attempts)attempt.cancel(true);}
            if(reset!=null)throw reset;
            if(android.os.SystemClock.elapsedRealtime()>=until)throw new IOException("Station connection unavailable; reconnecting");
            try{Thread.sleep(75);}catch(InterruptedException error){Thread.currentThread().interrupt();throw new InterruptedIOException();}
        }
        throw new InterruptedIOException("Listening stopped");
    }
    @Override public int read(byte[] buffer,int start,int length)throws IOException {
        if(length==0)return 0;
        if(closed)throw new InterruptedIOException("Listening stopped");
        if(at==pending.length){pending=next();at=0;sequence++;}
        int count=Math.min(length,pending.length-at);System.arraycopy(pending,at,buffer,start,count);at+=count;offset+=count;bytesTransferred(count);return count;
    }
    @Override public Uri getUri(){return uri;}
    @Override public void close(){closed=true;if(listener.equals(statusOwner))connectionStatus="";for(Call call:calls)call.cancel();if(workers!=null)workers.shutdownNow();if(started){started=false;transferEnded();}}
}
