package cloud.gitwide.ampverse;

import android.content.ComponentName;
import android.content.Context;
import android.net.Uri;
import androidx.media3.common.MediaItem;
import androidx.media3.session.MediaController;
import androidx.media3.session.SessionToken;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.List;
import java.util.concurrent.TimeUnit;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public class RadioDeviceTest {
    private void main(Runnable action){InstrumentationRegistry.getInstrumentation().runOnMainSync(action);}
    @Test public void playsHttpAudioThroughRealDspPipeline()throws Exception{
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();List<MediaItem> saved=LibraryStore.read(context);
        int size=44100*2*8;ByteBuffer wave=ByteBuffer.allocate(44+size).order(ByteOrder.LITTLE_ENDIAN);
        wave.put("RIFF".getBytes()).putInt(36+size).put("WAVEfmt ".getBytes()).putInt(16).putShort((short)1).putShort((short)1).putInt(44100).putInt(88200).putShort((short)2).putShort((short)16).put("data".getBytes()).putInt(size);
        for(int i=0;i<size/2;i++)wave.putShort((short)(Math.sin(2*Math.PI*1000*i/44100)*4000));
        MediaController[] player=new MediaController[1];
        try(ServerSocket server=new ServerSocket(0,1,java.net.InetAddress.getByName("127.0.0.1"));ActivityScenario<MainActivity> scenario=ActivityScenario.launch(MainActivity.class)){
            Thread responder=new Thread(()->{try(Socket socket=server.accept()){
                socket.getOutputStream().write(("HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: "+wave.capacity()+"\r\nConnection: close\r\n\r\n").getBytes());socket.getOutputStream().write(wave.array());socket.getOutputStream().flush();
            }catch(Exception ignored){}});responder.start();
            com.google.common.util.concurrent.ListenableFuture<MediaController>[] pending=new com.google.common.util.concurrent.ListenableFuture[1];
            main(()->pending[0]=new MediaController.Builder(context,new SessionToken(context,new ComponentName(context,PlaybackService.class))).buildAsync());player[0]=pending[0].get(10,TimeUnit.SECONDS);
            main(()->{AudioDsp.visible=true;AudioDsp.spectrumTime=0;player[0].setMediaItem(new MediaItem.Builder().setMediaId("amp-radio").setUri(Uri.parse("http://127.0.0.1:"+server.getLocalPort()+"/fixture.wav")).build());player[0].setVolume(0);player[0].prepare();player[0].play();});
            Thread.sleep(1800);
            main(()->{assertTrue(player[0].isPlaying());assertTrue(player[0].getCurrentPosition()>400);assertTrue(AudioDsp.spectrumTime>0);});
            responder.join(3000);
        }finally{if(player[0]!=null)main(()->{player[0].stop();player[0].setMediaItems(saved);player[0].setVolume(1);player[0].release();});AudioDsp.visible=false;}
    }
}
