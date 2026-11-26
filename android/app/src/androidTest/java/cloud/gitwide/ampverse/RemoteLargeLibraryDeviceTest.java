package cloud.gitwide.ampverse;

import android.content.*;
import android.view.*;
import android.widget.*;
import androidx.test.core.app.ActivityScenario;
import androidx.test.platform.app.InstrumentationRegistry;
import org.json.*;
import org.junit.Test;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.lang.reflect.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import static org.junit.Assert.*;

public class RemoteLargeLibraryDeviceTest {
    @Test public void live_host_connects_and_lists_playlists_without_full_catalog()throws Exception {
        String base=InstrumentationRegistry.getArguments().getString("controlUrl");
        org.junit.Assume.assumeNotNull(base);
        JSONObject handshake=RemotePanel.handshake(base,"");
        String session=handshake.getJSONObject("control").getString("session");
        JSONObject lists=RemotePanel.request(base,"","/api/v1/playlists",null,session);
        assertEquals(session,lists.getString("session"));assertTrue(lists.getJSONArray("playlists").length()>0);
        JSONObject reply=RemotePanel.request(base,"","/api/v1/command",new JSONObject().put("id","remote-test-"+UUID.randomUUID()).put("action","adjust_volume").put("delta",0),session);
        assertTrue(reply.getBoolean("ok"));
        InstrumentationRegistry.getInstrumentation().getTargetContext().getSharedPreferences("settings",0).edit().putString("remote-server",base).commit();
    }
    static final class Host implements AutoCloseable {
        final ServerSocket server=new ServerSocket(0,32,InetAddress.getByName("127.0.0.1"));
        final ExecutorService clients=Executors.newCachedThreadPool();
        final AtomicInteger catalogs=new AtomicInteger();
        final List<JSONObject> commands=new CopyOnWriteArrayList<>();
        final CountDownLatch searching=new CountDownLatch(1),releaseSearch=new CountDownLatch(1),done=new CountDownLatch(1);
        volatile boolean closed;
        Host()throws Exception {new Thread(()->{while(!closed)try{Socket socket=server.accept();clients.submit(()->reply(socket));}catch(IOException ignored){}}).start();}
        String base(){return "http://127.0.0.1:"+server.getLocalPort();}
        void reply(Socket socket){try(Socket connection=socket){
            BufferedReader input=new BufferedReader(new InputStreamReader(connection.getInputStream(),StandardCharsets.UTF_8));
            String first=input.readLine();if(first==null)return;String path=first.split(" ")[1],line,key="";int size=0;
            while((line=input.readLine())!=null&&!line.isEmpty()){if(line.toLowerCase(Locale.ROOT).startsWith("content-length:"))size=Integer.parseInt(line.split(":",2)[1].trim());if(line.toLowerCase(Locale.ROOT).startsWith("sec-websocket-key:"))key=line.split(":",2)[1].trim();}
            if(path.equals("/api/v1/events")){
                String accept=Base64.getEncoder().encodeToString(java.security.MessageDigest.getInstance("SHA-1").digest((key+"258EAFA5-E914-47DA-95CA-C5AB0DC85B11").getBytes(StandardCharsets.US_ASCII)));
                connection.getOutputStream().write(("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: "+accept+"\r\n\r\n").getBytes(StandardCharsets.US_ASCII));connection.getOutputStream().flush();done.await(10,TimeUnit.SECONDS);return;
            }
            JSONObject result;
            if(path.equals("/api/v1/info"))result=new JSONObject("{\"protocol\":1,\"service\":\"amp-control\",\"session\":\"large\"}");
            else if(path.equals("/api/v1/state"))result=new JSONObject("{\"control\":{\"session\":\"large\"},\"amp\":{\"playback_owner\":\"dj\",\"playback_state\":\"playing\",\"current_song\":\"Fixture\"}}");
            else if(path.equals("/api/v1/catalog")){catalogs.incrementAndGet();throw new IOException("Full catalog deliberately unavailable: library exceeds old size limit");}
            else if(path.equals("/api/v1/playlists"))result=new JSONObject("{\"session\":\"large\",\"playlists\":[{\"id\":\"rock\",\"name\":\"Rock\",\"kind\":\"smart\"},{\"id\":\"regular_party\",\"name\":\"Party\",\"kind\":\"playlist\"}]}");
            else if(path.startsWith("/api/v1/resources?")){searching.countDown();releaseSearch.await(5,TimeUnit.SECONDS);result=new JSONObject("{\"session\":\"large\",\"songs\":[],\"total\":0}");}
            else if(path.equals("/api/v1/command")){char[] body=new char[size];int at=0;while(at<size){int n=input.read(body,at,size-at);if(n<0)break;at+=n;}commands.add(new JSONObject(new String(body)));result=new JSONObject("{\"ok\":true}");}
            else result=new JSONObject();
            byte[] bytes=result.toString().getBytes(StandardCharsets.UTF_8);OutputStream out=connection.getOutputStream();out.write(("HTTP/1.1 200 OK\r\nContent-Length: "+bytes.length+"\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));out.write(bytes);out.flush();
        }catch(Exception ignored){}}
        public void close()throws Exception {closed=true;releaseSearch.countDown();done.countDown();server.close();clients.shutdownNow();}
    }
    static Object field(Object target,String name)throws Exception {Field f=target.getClass().getDeclaredField(name);f.setAccessible(true);return f.get(target);}
    static void invoke(Object target,String name)throws Exception {Method m=target.getClass().getDeclaredMethod(name);m.setAccessible(true);m.invoke(target);}
    static Button find(View view,String title){if(view instanceof Button&&((Button)view).getText().toString().equals(title))return (Button)view;if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++){Button b=find(group.getChildAt(i),title);if(b!=null)return b;}}return null;}
    @Test public void controls_and_playlists_work_without_catalog_and_during_slow_search()throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();RemotePanel[] panel={null};
        String previous=context.getSharedPreferences("settings",0).getString("remote-server","");
        try(Host host=new Host();ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class))){
            scenario.onActivity(a->{try{panel[0]=new RemotePanel(a,new Appearance(a));((EditText)field(panel[0],"address")).setText(host.base());invoke(panel[0],"connect");}catch(Exception e){throw new RuntimeException(e);}});
            AtomicBoolean connected=new AtomicBoolean();long until=System.currentTimeMillis()+5000;
            while(System.currentTimeMillis()<until&&!connected.get()){Thread.sleep(50);scenario.onActivity(a->{try{connected.set((boolean)field(panel[0],"connected"));}catch(Exception e){throw new RuntimeException(e);}});}
            assertTrue("Transport must connect without fetching the full catalog",connected.get());assertEquals(0,host.catalogs.get());
            scenario.onActivity(a->{try{((EditText)field(panel[0],"search")).setText("abc");}catch(Exception e){throw new RuntimeException(e);}});
            assertTrue(host.searching.await(3,TimeUnit.SECONDS));
            scenario.onActivity(a->{assertNotNull(find(panel[0],"Pause"));find(panel[0],"Pause").performClick();});
            until=System.currentTimeMillis()+2000;while(host.commands.isEmpty()&&System.currentTimeMillis()<until)Thread.sleep(30);
            assertFalse("Slow song search must not block Pause",host.commands.isEmpty());assertEquals("pause",host.commands.get(0).getString("action"));assertEquals("auto",host.commands.get(0).getString("deck"));
            host.releaseSearch.countDown();
            scenario.onActivity(a->find(panel[0],"Change playlist").performClick());
            AtomicInteger count=new AtomicInteger();until=System.currentTimeMillis()+3000;while(count.get()<2&&System.currentTimeMillis()<until){Thread.sleep(50);scenario.onActivity(a->{try{count.set(((JSONArray)field(panel[0],"playlists")).length());}catch(Exception e){throw new RuntimeException(e);}});}
            assertEquals("Both Smart and regular playlist choices must load independently",2,count.get());assertEquals(0,host.catalogs.get());
        }finally{if(panel[0]!=null)InstrumentationRegistry.getInstrumentation().runOnMainSync(()->panel[0].close());context.getSharedPreferences("settings",0).edit().putString("remote-server",previous).commit();}
    }
}
