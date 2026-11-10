package cloud.gitwide.ampverse;
import android.content.*;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.uiautomator.*;
import org.junit.*;
import org.junit.runner.RunWith;
import java.io.*;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
public class DjPhoneLayoutDeviceTest {
    private android.view.View find(android.view.View view,String description){if(description.contentEquals(view.getContentDescription()==null?"":view.getContentDescription()))return view;if(view instanceof android.view.ViewGroup){android.view.ViewGroup group=(android.view.ViewGroup)view;for(int i=0;i<group.getChildCount();i++){android.view.View match=find(group.getChildAt(i),description);if(match!=null)return match;}}return null;}
    @Test public void threeThemesKeepDecksAndSquarePadsReachable()throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();SharedPreferences prefs=context.getSharedPreferences("settings",0);String original=prefs.getString("theme","classic");UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
        try{for(String theme:new String[]{"classic","modern-light","modern-dark"}){
            prefs.edit().putString("theme",theme).commit();Intent intent=new Intent(context,MainActivity.class).putExtra("module","AMP DJ").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(intent)){
                scenario.onActivity(a->{a.getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);a.setShowWhenLocked(true);a.setTurnScreenOn(true);});
                assertTrue(device.wait(Until.hasObject(By.desc("DECK B")),5000));device.findObject(By.desc("DECK B")).click();assertTrue(device.wait(Until.hasObject(By.text("DECK B")),2000));
                android.graphics.Bitmap screenshot=InstrumentationRegistry.getInstrumentation().getUiAutomation().takeScreenshot();if(screenshot!=null)try(FileOutputStream output=new FileOutputStream(new File(context.getExternalFilesDir(null),"dj-"+theme+".png"))){screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,output);}
                UiScrollable scroll=new UiScrollable(new UiSelector().scrollable(true));scroll.scrollTextIntoView("Left side deck");
                // Choose Sampler mode through the view tree, without touching transport state.
                scenario.onActivity(a->{DjEngine engine=PlaybackService.dj();assertNotNull(engine);engine.mode(0,false);if(engine.collapsed[0])engine.collapse(0);});
                // Rebuild this module so persisted panel mode is reflected in its views.
                scenario.onActivity(a->a.onNewIntent(new Intent(a,MainActivity.class).putExtra("module","AMP DJ")));
                scroll=new UiScrollable(new UiSelector().scrollable(true));scroll.scrollIntoView(new UiSelector().description("Left pad 1"));UiObject pad=new UiObject(new UiSelector().description("Left pad 1"));assertTrue(pad.exists());scenario.onActivity(a->{android.view.View view=find(a.getWindow().getDecorView(),"Left pad 1");assertNotNull(view);assertEquals("Sampler pad must be square",view.getWidth(),view.getHeight());});
                assertTrue(scroll.scrollIntoView(new UiSelector().description("Search DJ music")));
            }
        }}finally{prefs.edit().putString("theme",original).commit();}
    }
}
