package cloud.gitwide.ampverse;
import android.net.Uri;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import java.io.*;
import java.net.*;
import java.util.concurrent.*;
import okhttp3.*;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
public final class HlsCastDeviceTest {
    @Test public void masterAndSegmentsCarryOnlySelectedIdentityAndNeverFollowRedirects()throws Exception{
        try(ServerSocket server=new ServerSocket(0,16,InetAddress.getByName("127.0.0.1"))){
            ExecutorService worker=Executors.newSingleThreadExecutor();
            Future<Boolean> headers=worker.submit(()->{
                for(int i=0;i<2;i++)try(Socket socket=server.accept()){
                    BufferedReader input=new BufferedReader(new InputStreamReader(socket.getInputStream(),java.nio.charset.StandardCharsets.US_ASCII));
                    String first=input.readLine(),line;boolean station=false,identity=false,password=false;
                    while((line=input.readLine())!=null&&!line.isEmpty()){
                        String lower=line.toLowerCase(java.util.Locale.ROOT);station|=lower.equals("x-amp-station: station");identity|=lower.equals("x-amp-server: server");password|=lower.startsWith("x-amp-password:");
                    }
                    if(!station||!identity||password||first==null)return false;
                    socket.getOutputStream().write("HTTP/1.1 302 Found\r\nLocation: http://example.invalid/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".getBytes(java.nio.charset.StandardCharsets.US_ASCII));
                }
                return true;
            });
            String base="http://127.0.0.1:"+server.getLocalPort();
            OkHttpClient client=StationMediaSourceFactory.stationHttp(Uri.parse(base+"/api/audio/hls/master.m3u8?server=server&station=station"));
            try{
                for(String path:new String[]{"/api/audio/hls/master.m3u8","/api/audio/hls/epoch/balanced/segment-1.m4s"})try(Response response=client.newCall(new Request.Builder().url(base+path).build()).execute()){assertEquals(302,response.code());}
                assertTrue(headers.get(3,TimeUnit.SECONDS));
                try{client.newCall(new Request.Builder().url(base+"/api/start").build()).execute();fail("Outside station HLS scope");}catch(IOException expected){}
            }finally{worker.shutdownNow();client.dispatcher().executorService().shutdown();client.connectionPool().evictAll();}
        }
    }
    static final class ToneServer implements AutoCloseable {
        final ServerSocket server=new ServerSocket(0,16,InetAddress.getByName("127.0.0.1"));
        final ExecutorService clients=Executors.newFixedThreadPool(3);
        final android.content.res.AssetManager assets=androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().getContext().getAssets();
        final java.util.concurrent.atomic.AtomicInteger rejected=new java.util.concurrent.atomic.AtomicInteger();
        volatile int bytesPerSecond=1000000;volatile boolean closed;
        final Thread accept;
        ToneServer()throws Exception{accept=new Thread(()->{while(!closed)try{Socket socket=server.accept();clients.submit(()->reply(socket));}catch(IOException ignored){}});accept.start();}
        String uri(){return "http://127.0.0.1:"+server.getLocalPort()+"/api/audio/hls/master.m3u8?server=server&station=station";}
        void reply(Socket socket){try(Socket connection=socket){
            BufferedReader input=new BufferedReader(new InputStreamReader(connection.getInputStream(),java.nio.charset.StandardCharsets.US_ASCII));String first=input.readLine(),line;boolean station=false,identity=false;
            while((line=input.readLine())!=null&&!line.isEmpty()){station|=line.equalsIgnoreCase("x-amp-station: station");identity|=line.equalsIgnoreCase("x-amp-server: server");}
            if(!station||!identity||first==null){rejected.incrementAndGet();return;}
            String path=new java.net.URI(first.split(" ")[1]).getPath().substring("/api/audio/hls/".length());
            if(path.contains(".."))throw new IOException("Unsafe fixture path");
            byte[] body;try(InputStream asset=assets.open("hls-tone/"+path);ByteArrayOutputStream bytes=new ByteArrayOutputStream()){byte[] buffer=new byte[4096];int count;while((count=asset.read(buffer))!=-1)bytes.write(buffer,0,count);body=bytes.toByteArray();}
            String type=path.endsWith(".m3u8")?"application/vnd.apple.mpegurl":"video/mp4";
            OutputStream output=connection.getOutputStream();output.write(("HTTP/1.1 200 OK\r\nContent-Type: "+type+"\r\nContent-Length: "+body.length+"\r\nConnection: close\r\n\r\n").getBytes(java.nio.charset.StandardCharsets.US_ASCII));
            for(int at=0;at<body.length&&!closed;){int count=Math.min(1024,body.length-at);output.write(body,at,count);output.flush();at+=count;if(path.endsWith(".m4s"))Thread.sleep(Math.max(1,1000L*count/bytesPerSecond));}
        }catch(Exception ignored){}}
        @Override public void close()throws Exception{closed=true;server.close();clients.shutdownNow();accept.join(2000);}
    }
    @Test public void generatedAacVariantsDecodeAndAdaptAfterBandwidthDrops()throws Exception{
        android.content.Context context=androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().getTargetContext();
        java.util.function.Consumer<Runnable> main=action->androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().runOnMainSync(action);
        androidx.media3.exoplayer.ExoPlayer[] player={null};java.util.Set<Integer> observed=java.util.concurrent.ConcurrentHashMap.newKeySet();
        java.util.concurrent.atomic.AtomicReference<androidx.media3.common.PlaybackException> failure=new java.util.concurrent.atomic.AtomicReference<>();
        try(ToneServer server=new ToneServer()){
            main.accept(()->{
                player[0]=new androidx.media3.exoplayer.ExoPlayer.Builder(context).setMediaSourceFactory(new StationMediaSourceFactory(context))
                    .setBandwidthMeter(new androidx.media3.exoplayer.upstream.DefaultBandwidthMeter.Builder(context).setInitialBitrateEstimate(1500000).build())
                    .setTrackSelector(new androidx.media3.exoplayer.trackselection.DefaultTrackSelector(context,new androidx.media3.exoplayer.trackselection.AdaptiveTrackSelection.Factory(1000,2000,1000,.7f)))
                    .setLoadControl(new androidx.media3.exoplayer.DefaultLoadControl.Builder().setBufferDurationsMs(1500,4000,1000,1000).build()).build();
                player[0].setVolume(0);
                player[0].addAnalyticsListener(new androidx.media3.exoplayer.analytics.AnalyticsListener(){@Override public void onDownstreamFormatChanged(EventTime time,androidx.media3.exoplayer.source.MediaLoadData data){if(data.trackFormat!=null&&data.trackFormat.averageBitrate>0)observed.add(data.trackFormat.averageBitrate);}});
                player[0].addListener(new androidx.media3.common.Player.Listener(){@Override public void onPlayerError(androidx.media3.common.PlaybackException error){failure.set(error);}});
                player[0].setMediaItem(new androidx.media3.common.MediaItem.Builder().setMediaId("amp-radio").setUri(server.uri()).build());player[0].prepare();player[0].play();
            });
            try{
                long until=System.currentTimeMillis()+15000;while(System.currentTimeMillis()<until&&observed.stream().noneMatch(rate->rate>=190000)&&failure.get()==null)Thread.sleep(100);
                assertNull(failure.get());assertTrue("Generated high-quality AAC must decode",observed.stream().anyMatch(rate->rate>=190000));
                server.bytesPerSecond=18000;
                until=System.currentTimeMillis()+20000;while(System.currentTimeMillis()<until&&observed.stream().noneMatch(rate->rate<=100000)&&failure.get()==null)Thread.sleep(100);
                assertNull(failure.get());assertTrue("Media3 must select the resilient variant on the throttled link",observed.stream().anyMatch(rate->rate<=100000));
                assertEquals("Every HLS request must retain station identity",0,server.rejected.get());
                main.accept(()->assertTrue("Generated audio must advance",player[0].getCurrentPosition()>1000));
            }finally{main.accept(()->player[0].release());}
        }
    }

}
