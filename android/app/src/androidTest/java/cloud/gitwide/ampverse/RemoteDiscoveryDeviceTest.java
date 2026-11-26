package cloud.gitwide.ampverse;

import android.content.Intent;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.uiautomator.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public class RemoteDiscoveryDeviceTest {
    @Test public void controlReplyUsesSenderAndRetainsPasswordRequirement() throws Exception {
        byte[] bytes=new JSONObject().put("protocol",1).put("service","amp-control")
            .put("session","test-session").put("name","Test desktop").put("port",8790)
            .put("url","http://untrusted.invalid").put("password_required",true)
            .toString().getBytes(StandardCharsets.UTF_8);
        ControlDiscovery.Server host=ControlDiscovery.parse(bytes,bytes.length,InetAddress.getByName("127.0.0.1"));
        assertEquals("http://127.0.0.1:8790",host.url);
        assertTrue(host.passwordRequired);
        try{ControlDiscovery.parse("{}".getBytes(StandardCharsets.UTF_8),2,InetAddress.getLoopbackAddress());fail("Malformed reply accepted");}
        catch(java.io.IOException expected){}
    }

    @Test public void remoteAutomaticallyFindsHostsAndRequiresExplicitPasswordConnect() throws Exception {
        android.content.Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
        device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
        try(DatagramSocket socket=new DatagramSocket(new InetSocketAddress(InetAddress.getByName("127.0.0.1"),ControlDiscovery.PORT))){
            socket.setSoTimeout(200);
            Thread responder=new Thread(()->{
                try{while(!Thread.currentThread().isInterrupted()&&!socket.isClosed()){
                    byte[] input=new byte[256];DatagramPacket query=new DatagramPacket(input,input.length);
                    try{socket.receive(query);}catch(SocketTimeoutException timeout){continue;}
                    if(!new String(query.getData(),0,query.getLength(),StandardCharsets.UTF_8).equals("AMPVERSE_CONTROL_DISCOVER_V1"))continue;
                    byte[] reply=new JSONObject().put("protocol",1).put("service","amp-control")
                        .put("session","ui-discovery-test").put("name","Discovery test desktop")
                        .put("port",8799).put("password_required",true).toString().getBytes(StandardCharsets.UTF_8);
                    socket.send(new DatagramPacket(reply,reply.length,query.getAddress(),query.getPort()));
                }}catch(Exception ignored){}
            });responder.start();
            try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class).putExtra("module","AMP Remote").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))){
                assertTrue(device.wait(Until.hasObject(By.desc("Discover AMP Verse devices")),5000));
                assertTrue(device.wait(Until.hasObject(By.textContains("Discovery test desktop · Player / DJ")),9000));
                assertEquals(1,device.findObjects(By.textContains("Discovery test desktop · Player / DJ")).size());
                device.findObject(By.textContains("Discovery test desktop · Player / DJ")).click();
                assertTrue(device.wait(Until.hasObject(By.textContains("Enter its control password")),2000));
                assertTrue(device.hasObject(By.text("http://127.0.0.1:8799")));
            }finally{responder.interrupt();socket.close();responder.join(1500);}
        }
    }
}
