package cloud.gitwide.ampverse;

import android.content.*;
import android.content.pm.ActivityInfo;
import android.graphics.Rect;
import android.view.*;
import android.widget.*;
import androidx.test.core.app.ActivityScenario;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.uiautomator.UiDevice;
import org.junit.Test;
import java.io.File;
import static org.junit.Assert.*;

public class CleanUiDeviceTest {
    private View text(View root,String label){if(root instanceof TextView&&label.contentEquals(((TextView)root).getText()))return root;if(root instanceof ViewGroup){ViewGroup group=(ViewGroup)root;for(int i=0;i<group.getChildCount();i++){View found=text(group.getChildAt(i),label);if(found!=null)return found;}}return null;}
    private View description(View root,String label){if(label.contentEquals(root.getContentDescription()==null?"":root.getContentDescription()))return root;if(root instanceof ViewGroup){ViewGroup group=(ViewGroup)root;for(int i=0;i<group.getChildCount();i++){View found=description(group.getChildAt(i),label);if(found!=null)return found;}}return null;}
    @Test public void navigationAndPrimaryControlsFitPortraitAndLandscape() throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class))){
            scenario.onActivity(a->{a.setShowWhenLocked(true);a.setTurnScreenOn(true);a.getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);});
            for(String module:new String[]{"player","library","dj","cast","remote"}){
                scenario.onActivity(a->{View nav=a.getWindow().getDecorView().findViewWithTag("nav-"+module);assertNotNull(nav);nav.performClick();});
                InstrumentationRegistry.getInstrumentation().waitForIdleSync();Thread.sleep(200);
                device.takeScreenshot(new File(context.getExternalFilesDir(null),"clean-"+module+".png"));
                scenario.onActivity(a->{View root=a.getWindow().getDecorView();for(String name:new String[]{"player","library","dj","cast","remote"}){View nav=root.findViewWithTag("nav-"+name);Rect bounds=new Rect();assertTrue(name,nav.getGlobalVisibleRect(bounds));assertTrue(name+" "+bounds+" / "+root.getWidth(),bounds.left>=0&&bounds.right<=root.getWidth());assertTrue(name+" height "+bounds.height(),bounds.height()+1>=44*a.getResources().getDisplayMetrics().density);assertEquals(name.equals(module),nav.isSelected());}});
                device.takeScreenshot(new File(context.getExternalFilesDir(null),"clean-"+module+".png"));
            }
            scenario.onActivity(a->{a.getWindow().getDecorView().findViewWithTag("nav-player").performClick();a.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE);});
            Thread.sleep(800);InstrumentationRegistry.getInstrumentation().waitForIdleSync();
            scenario.onActivity(a->{View root=a.getWindow().getDecorView();for(String name:new String[]{"player","library","dj","cast","remote"}){View tab=root.findViewWithTag("nav-"+name);Rect bounds=new Rect();assertTrue(tab.getGlobalVisibleRect(bounds));assertTrue(bounds.right<=root.getWidth());}assertNotNull(description(root,"Play or pause"));});
            device.takeScreenshot(new File(context.getExternalFilesDir(null),"clean-player-landscape.png"));
        }
    }
    @Test public void libraryAndDjExposeSimpleActionsAndRetainAdvancedTools() throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class).putExtra("module","AMP Library"))){
            scenario.onActivity(a->{a.setShowWhenLocked(true);a.setTurnScreenOn(true);a.getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);});
            Thread.sleep(300);scenario.onActivity(a->{View root=a.getWindow().getDecorView();assertNotNull(text(root,"+ Add music"));View playlists=text(root,"Playlists");assertNotNull(playlists);playlists.performClick();});
            scenario.onActivity(a->{View root=a.getWindow().getDecorView();assertNotNull(text(root,"+ Smart Playlist"));root.findViewWithTag("nav-dj").performClick();});
            assertTrue(UiDevice.getInstance(InstrumentationRegistry.getInstrumentation()).wait(androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.desc("Phone Auto Mix")),5000));scenario.onActivity(a->{View root=a.getWindow().getDecorView();View auto=description(root,"Phone Auto Mix");assertTrue("Auto Mix view: "+auto,auto instanceof Switch);assertNotNull(text(root,"Choose playlist"));View advanced=description(root,"Sampler & Cue Decks");assertNotNull(advanced);advanced.performClick();assertNotNull(description(root,"Left side deck"));});
        }
    }
}
