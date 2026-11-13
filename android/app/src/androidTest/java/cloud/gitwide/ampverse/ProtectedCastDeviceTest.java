package cloud.gitwide.ampverse;

import android.net.Uri;
import androidx.media3.datasource.DataSpec;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import org.junit.*;
import org.junit.runner.RunWith;
import org.json.*;
import java.io.*;
import java.net.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
public final class ProtectedCastDeviceTest {
    static final byte[][] AUDIO={new byte[8192],new byte[8192],new byte[8192]};
    static{for(int i=0;i<AUDIO.length;i++)Arrays.fill(AUDIO[i],(byte)(i+1));}
    static String hash(byte[] bytes)throws Exception{StringBuilder out=new StringBuilder();for(byte b:MessageDigest.getInstance("SHA-256").digest(bytes))out.append(String.format(Locale.ROOT,"%02x",b&255));return out.toString();}
    static final class Link implements AutoCloseable {
        final ServerSocket server=new ServerSocket(0,32,InetAddress.getByName("127.0.0.1"));
        final ExecutorService clients=Executors.newCachedThreadPool();
        final AtomicInteger chunks=new AtomicInteger();
        final Thread thread;
        volatile boolean closed,broken,wrongStation;
        volatile String alternate;
        Link()throws Exception{thread=new Thread(()->{while(!closed)try{Socket socket=server.accept();clients.submit(()->reply(socket));}catch(IOException ignored){}});thread.start();}
        String base(){return "http://127.0.0.1:"+server.getLocalPort();}
        void reply(Socket socket){try(Socket connection=socket){
            connection.setSoTimeout(3000);BufferedReader input=new BufferedReader(new InputStreamReader(connection.getInputStream(),java.nio.charset.StandardCharsets.US_ASCII));
            String first=input.readLine();if(first==null)return;String line;while((line=input.readLine())!=null&&!line.isEmpty()){}
            String path=first.split(" ")[1];byte[] body;String extra="";
            if(path.startsWith("/api/audio/manifest")){
                JSONArray paths=new JSONArray();if(alternate!=null)paths.put(alternate);
                body=new JSONObject().put("protocol",1).put("server_id",wrongStation?"wrong":"server").put("station_id","station").put("epoch","epoch").put("start_sequence",0).put("paths",paths).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);
            }else{
                int index=Integer.parseInt(path.substring(path.lastIndexOf('/')+1,path.indexOf('?')));chunks.incrementAndGet();
                if(broken&&index==2)return;
                body=AUDIO[index];int reported=broken&&index==1?0:index;
                extra="X-Amp-Epoch: epoch\r\nX-Amp-Sequence: "+reported+"\r\nX-Amp-Sha256: "+hash(body)+"\r\n";
            }
            OutputStream output=connection.getOutputStream();output.write(("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: "+body.length+"\r\n"+extra+"\r\n").getBytes(java.nio.charset.StandardCharsets.US_ASCII));output.write(body);output.flush();
        }catch(Exception ignored){}}
        public void close()throws Exception{closed=true;server.close();clients.shutdownNow();thread.join(2000);}
    }
    @Test public void secondary_recovers_primary_loss_and_duplicate_wrong_sequence_is_never_played()throws Exception {
        try(Link primary=new Link();Link secondary=new Link()){
            primary.alternate=secondary.base();primary.broken=true;
            ProtectedCastSource source=new ProtectedCastSource();
            try{
                source.open(new DataSpec(Uri.parse(primary.base()+"/amp-protected.mp3?server=server&station=station")));
                byte[] output=new byte[8192*3];int at=0;
                while(at<output.length){int count=source.read(output,at,output.length-at);assertTrue(count>0);at+=count;}
                for(int i=0;i<3;i++)assertArrayEquals(AUDIO[i],Arrays.copyOfRange(output,i*8192,(i+1)*8192));
                assertTrue("Backup must supply lost/corrupt chunks",secondary.chunks.get()>0);
            }finally{source.close();}
        }
    }
    @Test public void identity_and_checksum_validation_reject_wrong_audio()throws Exception {
        try{ProtectedCastSource.validate(AUDIO[0],0,"epoch","1","epoch",hash(AUDIO[0]));fail("Wrong sequence");}catch(IOException expected){}
        try{ProtectedCastSource.validate(AUDIO[0],0,"epoch","0","other",hash(AUDIO[0]));fail("Wrong epoch");}catch(IOException expected){}
        try{ProtectedCastSource.validate(AUDIO[0],0,"epoch","0","epoch","bad");fail("Wrong checksum");}catch(IOException expected){}
        assertFalse(ProtectedCastSource.safeAlternative("http://192.168.1.2:8787","http://user:secret@192.168.1.3:8787"));
        assertFalse(ProtectedCastSource.safeAlternative("https://192.168.1.2","http://192.168.1.3"));
    }
    @Test public void real_mp3_plays_through_service_without_rebuffering_during_path_failure()throws Exception {
        String url=androidx.test.platform.app.InstrumentationRegistry.getArguments().getString("protectedUrl");
        org.junit.Assume.assumeNotNull(url);
        android.content.Context context=androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().getTargetContext();
        java.util.function.Consumer<Runnable> main=action->androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().runOnMainSync(action);
        final androidx.media3.session.MediaController[] player=new androidx.media3.session.MediaController[1];
        final com.google.common.util.concurrent.ListenableFuture<androidx.media3.session.MediaController>[] future=new com.google.common.util.concurrent.ListenableFuture[1];
        main.accept(()->future[0]=new androidx.media3.session.MediaController.Builder(context,new androidx.media3.session.SessionToken(context,new android.content.ComponentName(context,PlaybackService.class))).buildAsync());
        player[0]=future[0].get(10,TimeUnit.SECONDS);
        List<androidx.media3.common.MediaItem> saved=new ArrayList<>();long[] position={0};int[] index={0};float[] volume={1};boolean[] priorPlaying={false};
        main.accept(()->{for(int i=0;i<player[0].getMediaItemCount();i++)saved.add(player[0].getMediaItemAt(i));position[0]=player[0].getCurrentPosition();index[0]=Math.max(0,player[0].getCurrentMediaItemIndex());volume[0]=player[0].getVolume();priorPlaying[0]=player[0].getPlayWhenReady();});
        AtomicBoolean monitoring=new AtomicBoolean();AtomicInteger stalls=new AtomicInteger();
        androidx.media3.common.Player.Listener observer=new androidx.media3.common.Player.Listener(){public void onPlaybackStateChanged(int state){if(monitoring.get()&&state==androidx.media3.common.Player.STATE_BUFFERING)stalls.incrementAndGet();}};
        try {
            main.accept(()->{player[0].setVolume(0);player[0].addListener(observer);player[0].setMediaItem(new androidx.media3.common.MediaItem.Builder().setMediaId("amp-radio").setUri(url).build());player[0].prepare();player[0].play();});
            long deadline=System.currentTimeMillis()+15000;boolean[] active={false};
            while(System.currentTimeMillis()<deadline){main.accept(()->active[0]=player[0].isPlaying());if(active[0])break;Thread.sleep(100);}
            assertTrue("Protected MP3 must start through the real phone audio service",active[0]);
            monitoring.set(true);Thread.sleep(12000);
            main.accept(()->{assertTrue(player[0].isPlaying());assertTrue(player[0].getCurrentPosition()>10000);});
            assertEquals("A recovered path failure must not drain the phone buffer",0,stalls.get());
        }finally{
            monitoring.set(false);
            main.accept(()->{player[0].removeListener(observer);player[0].pause();player[0].stop();player[0].setMediaItems(saved,index[0],position[0]);player[0].setVolume(volume[0]);if(priorPlaying[0]&&!saved.isEmpty()){player[0].prepare();player[0].play();}player[0].release();});
        }
    }
}
