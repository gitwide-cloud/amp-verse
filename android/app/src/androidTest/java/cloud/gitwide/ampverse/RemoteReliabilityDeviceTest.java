package cloud.gitwide.ampverse;

import android.content.*;
import androidx.test.core.app.ActivityScenario;
import androidx.test.platform.app.InstrumentationRegistry;
import org.json.*;
import org.junit.Test;
import java.net.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicInteger;
import java.lang.reflect.*;
import static org.junit.Assert.*;

public class RemoteReliabilityDeviceTest {
    @Test public void stateFromRestartedHostIsRejectedAndSessionIsSent() throws Exception {
        try(ServerSocket server=new ServerSocket(0,2,InetAddress.getByName("127.0.0.1"))){
            ExecutorService worker=Executors.newSingleThreadExecutor();
            Future<String> received=worker.submit(()->{try(Socket socket=server.accept()){socket.setSoTimeout(3000);BufferedReader reader=new BufferedReader(new InputStreamReader(socket.getInputStream(),StandardCharsets.UTF_8));StringBuilder request=new StringBuilder();String line;while((line=reader.readLine())!=null&&!line.isEmpty())request.append(line).append("\n");byte[] body="{\"control\":{\"session\":\"new-host\"}}".getBytes(StandardCharsets.UTF_8);socket.getOutputStream().write(("HTTP/1.1 200 OK\r\nContent-Length: "+body.length+"\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.UTF_8));socket.getOutputStream().write(body);return request.toString();}});
            try{RemotePanel.request("http://127.0.0.1:"+server.getLocalPort(),"","/api/v1/state",null,"old-host");fail("Accepted stale host state");}catch(IOException expected){assertTrue(expected.getMessage(),expected.getMessage().contains("Host session changed"));}
            assertTrue(received.get(5,TimeUnit.SECONDS).toLowerCase().contains("x-amp-session: old-host"));worker.shutdownNow();
        }
    }
    @Test public void automaticReconnectContinuesAfterASecondRefusedConnection() throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();int port;try(ServerSocket socket=new ServerSocket(0,1,InetAddress.getByName("127.0.0.1"))){port=socket.getLocalPort();}
        RemotePanel[] panel=new RemotePanel[1];AtomicInteger attempts=new AtomicInteger();
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class))){
            scenario.onActivity(a->{try{panel[0]=new RemotePanel(a,new Appearance(a));Field address=RemotePanel.class.getDeclaredField("address");address.setAccessible(true);((android.widget.EditText)address.get(panel[0])).setText("http://127.0.0.1:"+port);Method connect=RemotePanel.class.getDeclaredMethod("connect",boolean.class);connect.setAccessible(true);connect.invoke(panel[0],true);}catch(Exception e){throw new RuntimeException(e);}});
            long deadline=System.currentTimeMillis()+6000;
            while(System.currentTimeMillis()<deadline&&attempts.get()<2){Thread.sleep(100);scenario.onActivity(a->{try{Field field=RemotePanel.class.getDeclaredField("reconnectAttempts");field.setAccessible(true);attempts.set(field.getInt(panel[0]));}catch(Exception e){throw new RuntimeException(e);}});}
            assertTrue("Automatic reconnect stopped after its first failed handshake",attempts.get()>=2);
        }finally{if(panel[0]!=null)InstrumentationRegistry.getInstrumentation().runOnMainSync(()->panel[0].close());}
    }
    @Test public void disconnectDiscardsCommandsWaitingBehindAnInFlightRequest() throws Exception { queuedWorkInvalidated(false); }
    @Test public void connectionLossDiscardsQueuedCommandsBeforeReconnectDelay() throws Exception { queuedWorkInvalidated(true); }
    private void queuedWorkInvalidated(boolean lost) throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();CountDownLatch started=new CountDownLatch(1),release=new CountDownLatch(1);AtomicInteger sent=new AtomicInteger();RemotePanel[] panel=new RemotePanel[1];
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class))){
            Class<?> work=Class.forName("cloud.gitwide.ampverse.RemotePanel$Work");Method submit=RemotePanel.class.getDeclaredMethod("submit",work,java.util.function.Consumer.class);submit.setAccessible(true);
            Object blocking=java.lang.reflect.Proxy.newProxyInstance(work.getClassLoader(),new Class[]{work},(o,m,args)->{started.countDown();release.await(5,TimeUnit.SECONDS);return new JSONObject();});
            Object queued=java.lang.reflect.Proxy.newProxyInstance(work.getClassLoader(),new Class[]{work},(o,m,args)->{sent.incrementAndGet();return new JSONObject();});
            scenario.onActivity(a->{try{panel[0]=new RemotePanel(a,new Appearance(a));submit.invoke(panel[0],blocking,(java.util.function.Consumer<JSONObject>)v->{});submit.invoke(panel[0],queued,(java.util.function.Consumer<JSONObject>)v->{});}catch(Exception e){throw new RuntimeException(e);}});
            assertTrue(started.await(3,TimeUnit.SECONDS));
            scenario.onActivity(a->{try{if(lost){Field generation=RemotePanel.class.getDeclaredField("generation");generation.setAccessible(true);Method method=RemotePanel.class.getDeclaredMethod("lost",int.class,Throwable.class);method.setAccessible(true);method.invoke(panel[0],generation.getInt(panel[0]),new java.net.ConnectException("test outage"));}else{Method disconnect=RemotePanel.class.getDeclaredMethod("disconnect");disconnect.setAccessible(true);disconnect.invoke(panel[0]);}}catch(Exception e){throw new RuntimeException(e);}});
            release.countDown();Field field=RemotePanel.class.getDeclaredField("worker");field.setAccessible(true);((ExecutorService)field.get(panel[0])).submit(()->{}).get(3,TimeUnit.SECONDS);assertEquals(0,sent.get());
        }finally{release.countDown();if(panel[0]!=null)InstrumentationRegistry.getInstrumentation().runOnMainSync(()->panel[0].close());}
    }
}
