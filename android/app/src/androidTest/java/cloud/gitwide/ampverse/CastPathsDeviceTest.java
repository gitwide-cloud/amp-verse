package cloud.gitwide.ampverse;

import android.content.*;
import android.graphics.Rect;
import android.view.*;
import android.widget.*;
import androidx.test.core.app.ActivityScenario;
import androidx.test.core.app.ApplicationProvider;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.atomic.*;
import static org.junit.Assert.*;

/** Generated same-device catalog only: checks actual owner UI without starting or stopping audio. */
@RunWith(AndroidJUnit4.class)
public class CastPathsDeviceTest {
    private TextView find(View view,String text){
        if(view instanceof TextView&&text.contentEquals(((TextView)view).getText()))return (TextView)view;
        if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++){TextView result=find(group.getChildAt(i),text);if(result!=null)return result;}}
        return null;
    }
    private View described(View view,String text){
        if(text.contentEquals(view.getContentDescription()==null?"":view.getContentDescription()))return view;
        if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++){View result=described(group.getChildAt(i),text);if(result!=null)return result;}}
        return null;
    }
    private View tagged(View view,String tag){
        if(tag.equals(view.getTag()))return view;
        if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++){View result=tagged(group.getChildAt(i),tag);if(result!=null)return result;}}
        return null;
    }
    private <T extends View>T type(View view,Class<T> target){
        if(target.isInstance(view))return target.cast(view);
        if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++){T result=type(group.getChildAt(i),target);if(result!=null)return result;}}
        return null;
    }
    private void visible(View view,String label){
        assertNotNull(label,view);assertTrue(label+" must be shown",view.isShown());
        view.requestRectangleOnScreen(new Rect(0,0,view.getWidth(),view.getHeight()),true);
        Rect bounds=new Rect();assertTrue(label+" must occupy the visible viewport",view.getGlobalVisibleRect(bounds)&&!bounds.isEmpty());
    }
    private void assertFriendlyVisibleLabels(View view){
        if(view.isShown()&&view instanceof TextView){
            String text=((TextView)view).getText().toString();
            assertFalse("Ordinary label exposed a URL: "+text,text.contains("http://")||text.contains("https://"));
            assertFalse("Ordinary label exposed an address: "+text,text.matches("(?s).*\\b(?:\\d{1,3}\\.){3}\\d{1,3}\\b.*"));
        }
        if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++)assertFriendlyVisibleLabels(group.getChildAt(i));}
    }
    private void waitForLibrary(ActivityScenario<MainActivity> scenario)throws Exception{
        long until=System.nanoTime()+java.util.concurrent.TimeUnit.SECONDS.toNanos(12);
        AtomicBoolean shown=new AtomicBoolean();
        while(System.nanoTime()<until){
            scenario.onActivity(a->{CastLibraryView library=type(a.getWindow().getDecorView(),CastLibraryView.class);shown.set(library!=null&&library.isShown());});
            if(shown.get())return;
            Thread.sleep(50);
        }
        fail("Selecting a named server must open its Library");
    }
    private static final class CatalogFixture implements AutoCloseable {
        final ServerSocket server=new ServerSocket(0,8,CastTestAddress.privateAddress());
        final List<String> requests=Collections.synchronizedList(new ArrayList<>());
        final AtomicReference<Throwable> failure=new AtomicReference<>();
        final Thread thread;
        CatalogFixture()throws Exception{
            thread=new Thread(()->{
                while(!server.isClosed())try(Socket socket=server.accept()){
                    socket.setSoTimeout(3000);
                    BufferedReader reader=new BufferedReader(new InputStreamReader(socket.getInputStream(),StandardCharsets.US_ASCII));
                    String first=reader.readLine(),line;while((line=reader.readLine())!=null&&!line.isEmpty()){}
                    requests.add(first);String path=first.split(" ")[1];String body;
                    if(path.equals("/api/status"))body="{\"server_id\":\"paths-fixture\",\"station_id\":\"paths-station\",\"library_id\":\"paths-library\",\"station_name\":\"Generated catalog fixture\",\"password_required\":false,\"running\":false,\"on_air\":false,\"capabilities\":{\"player_queue\":1,\"player_track_refs\":\"library_path_v1\",\"private_audio\":\"library_path_v1\",\"playlist_snapshot\":\"library_path_v1\"}}";
                    else if(path.equals("/api/playlist"))body="{\"tracks\":[]}";
                    else if(path.startsWith("/api/studio/tracks?"))body="{\"tracks\":[],\"total\":0,\"offset\":0,\"limit\":50}";
                    else {failure.compareAndSet(null,new AssertionError("Unexpected fixture request: "+first));body="{}";}
                    byte[] bytes=body.getBytes(StandardCharsets.UTF_8);
                    OutputStream out=socket.getOutputStream();out.write(("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: "+bytes.length+"\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));out.write(bytes);out.flush();
                }catch(Throwable error){if(!server.isClosed())failure.compareAndSet(null,error);}
            },"cast-paths-generated-catalog");
            thread.start();
        }
        String address(){return "http://"+server.getInetAddress().getHostAddress()+":"+server.getLocalPort();}
        public void close()throws Exception{server.close();thread.join(3000);assertFalse("Catalog fixture must stop",thread.isAlive());}
    }
    @Test public void serverLibraryOutputsStayVisibleAndConnectionDetailsStayAdvanced()throws Exception{
        org.junit.Assume.assumeTrue("Full Cast UI only",BuildConfig.EDITION.equals("full"));
        Context context=ApplicationProvider.getApplicationContext();
        SharedPreferences settings=context.getSharedPreferences("settings",Context.MODE_PRIVATE),sections=context.getSharedPreferences("ui-sections",Context.MODE_PRIVATE);
        String[] keys={"cast-server","cast-server-id","cast-station-id"};Map<String,String> saved=new HashMap<>();
        for(String key:keys)saved.put(key,settings.getString(key,null));
        String sectionKey="MainActivity:Advanced · station source and diagnostics";boolean hadSection=sections.contains(sectionKey),expanded=sections.getBoolean(sectionKey,false);
        sections.edit().putBoolean(sectionKey,false).commit();
        try(CatalogFixture fixture=new CatalogFixture()){
            Intent intent=new Intent(context,MainActivity.class).putExtra("module","AMP Cast").putExtra("phone-ui",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(intent)){
                scenario.onActivity(activity->{
                    // Explicit foreground navigation also covers a restored Library default tab.
                    View root=activity.getWindow().getDecorView(),devices=tagged(root,"nav-devices");assertNotNull(devices);devices.performClick();
                    root=activity.getWindow().getDecorView();View cast=tagged(root,"nav-cast");assertNotNull(cast);if(!cast.isSelected())cast.performClick();
                    root=activity.getWindow().getDecorView();visible(find(root,"AMP Radio · choose a server"),"Named server selection");
                    CastLibraryView library=type(root,CastLibraryView.class);assertNotNull(library);assertFalse("Unselected server Library must stay hidden",library.isShown());
                    TextView connect=find(root,"Connect");assertNotNull(connect);assertFalse("Manual Connect starts in Advanced",connect.isShown());
                    View address=described(root,"AMP Cast server address"),password=described(root,"AMP Cast admin password");assertNotNull(address);assertNotNull(password);assertFalse(address.isShown());assertFalse(password.isShown());
                    for(String text:new String[]{"Listen to host on this phone","Send host stream to Chromecast","Request playback on server speakers","Start host station","Request desktop host Player/DJ master…"}){
                        TextView action=find(root,text);assertNotNull(text,action);assertFalse("Disconnected host action "+text,action.isEnabled());assertFalse("Legacy host action stays in Advanced "+text,action.isShown());
                    }
                    assertFriendlyVisibleLabels(root);
                    TextView advanced=find(root,"Advanced connection settings");visible(advanced,"Advanced disclosure");advanced.performClick();visible(address,"Advanced server address");visible(password,"Advanced optional access code");visible(connect,"Advanced Connect");
                    ((EditText)address).setText(fixture.address());((EditText)password).setText("");connect.performClick();
                    ((android.view.inputmethod.InputMethodManager)activity.getSystemService(Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(root.getWindowToken(),0);
                });
                waitForLibrary(scenario);
                InstrumentationRegistry.getInstrumentation().waitForIdleSync();
                scenario.onActivity(activity->{
                    View root=activity.getWindow().getDecorView();CastLibraryView library=type(root,CastLibraryView.class);visible(find(library,"Server Library"),"Selected server Library");
                    assertFalse("Address hides after selection",described(root,"AMP Cast server address").isShown());assertFalse("Connect hides after selection",find(root,"Connect").isShown());
                    Spinner output=type(library,Spinner.class);visible(output,"Library output selector");assertEquals("Exactly Phone and Server outputs",2,output.getAdapter().getCount());
                    assertEquals("This phone · AMP Player",output.getAdapter().getItem(0).toString());assertEquals("Selected server · speakers",output.getAdapter().getItem(1).toString());assertEquals(0,output.getSelectedItemPosition());
                    visible(find(library,"This phone · AMP Player"),"Selected Phone output");output.setSelection(1);
                });
                InstrumentationRegistry.getInstrumentation().waitForIdleSync();
                scenario.onActivity(activity->{
                    View root=activity.getWindow().getDecorView();CastLibraryView library=type(root,CastLibraryView.class);Spinner output=type(library,Spinner.class);assertEquals(1,output.getSelectedItemPosition());visible(find(library,"Selected server · speakers"),"Selected Server output");output.setSelection(0);
                    for(String text:new String[]{"Stop","Remote","Search","New playlist","New Smart"})visible(find(library,text),"Library action "+text);
                    View legacy=described(root,"Advanced · station source and diagnostics");visible(legacy,"Advanced station disclosure");TextView oldHostAction=find(root,"Start host station");assertFalse(oldHostAction.isShown());legacy.performClick();visible(oldHostAction,"Advanced station controls");legacy.performClick();assertFalse(oldHostAction.isShown());
                    assertFriendlyVisibleLabels(root);
                });
            }
            assertNull("Generated fixture had no unexpected requests",fixture.failure.get());
            synchronized(fixture.requests){assertFalse(fixture.requests.isEmpty());for(String request:fixture.requests)assertTrue("Browsing/output selection must not send playback or download commands: "+request,request.startsWith("GET "));}
        }finally{
            SharedPreferences.Editor restore=settings.edit();for(String key:keys)if(saved.get(key)==null)restore.remove(key);else restore.putString(key,saved.get(key));restore.commit();
            SharedPreferences.Editor restoreSection=sections.edit();if(hadSection)restoreSection.putBoolean(sectionKey,expanded);else restoreSection.remove(sectionKey);restoreSection.commit();
        }
    }
}
