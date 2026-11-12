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
    @Test public void leavingLibrarySearchDismissesKeyboard() throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class).putExtra("module","AMP Library"))){
            Thread.sleep(400);
            scenario.onActivity(a->{View input=description(a.getWindow().getDecorView(),"Search phone music");assertTrue(input instanceof EditText);input.requestFocus();((android.view.inputmethod.InputMethodManager)a.getSystemService(Context.INPUT_METHOD_SERVICE)).showSoftInput(input,android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT);});
            Thread.sleep(500);
            scenario.onActivity(a->a.getWindow().getDecorView().findViewWithTag("nav-dj").performClick());Thread.sleep(500);
            scenario.onActivity(a->{if(android.os.Build.VERSION.SDK_INT>=30)assertFalse(a.getWindow().getDecorView().getRootWindowInsets().isVisible(WindowInsets.Type.ime()));});
        }
    }
    private ScrollView scrolling(View view){if(view instanceof ScrollView)return (ScrollView)view;if(view instanceof ViewGroup)for(int i=0;i<((ViewGroup)view).getChildCount();i++){ScrollView found=scrolling(((ViewGroup)view).getChildAt(i));if(found!=null)return found;}return null;}
    @Test public void returningToDjPreservesBrowsingPosition() throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class).putExtra("module","AMP DJ"))){
            Thread.sleep(500);
            scenario.onActivity(a->{ScrollView scroll=scrolling(a.getWindow().getDecorView());assertNotNull(scroll);scroll.scrollTo(0,400);});
            int[] offset={0};scenario.onActivity(a->{offset[0]=scrolling(a.getWindow().getDecorView()).getScrollY();assertTrue(offset[0]>0);a.getWindow().getDecorView().findViewWithTag("nav-library").performClick();});
            scenario.onActivity(a->a.getWindow().getDecorView().findViewWithTag("nav-dj").performClick());
            InstrumentationRegistry.getInstrumentation().waitForIdleSync();
            scenario.onActivity(a->{View root=a.getWindow().getDecorView();assertEquals(offset[0],scrolling(root).getScrollY());Rect fader=new Rect();assertTrue("Pinned crossfader hidden",description(root,"DJ crossfader").getGlobalVisibleRect(fader));assertTrue(fader.height()+1>=48*a.getResources().getDisplayMetrics().density);});
        }
    }
    @Test public void largerTextKeepsNavigationAndTransportReachable() throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
        String original=device.executeShellCommand("settings get system font_scale").trim();if(!original.matches("[0-9.]+"))original="1.0";
        try {
            device.executeShellCommand("settings put system font_scale 1.5");
            try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class))){
                Thread.sleep(700);InstrumentationRegistry.getInstrumentation().waitForIdleSync();
                scenario.onActivity(a->{View root=a.getWindow().getDecorView();for(String name:new String[]{"player","library","dj","devices"}){View tab=root.findViewWithTag("nav-"+name);Rect rect=new Rect();assertTrue(tab.getGlobalVisibleRect(rect));assertTrue(rect.right<=root.getWidth());assertEquals("Clipped large text tab "+name,tab.getHeight(),rect.height());}Rect rect=new Rect();assertTrue(description(root,"Play or pause").getGlobalVisibleRect(rect));assertTrue(rect.height()+1>=48*a.getResources().getDisplayMetrics().density);});
                device.takeScreenshot(new File(context.getExternalFilesDir(null),"clean-large-text.png"));
            }
        }finally{device.executeShellCommand("settings put system font_scale "+original);}
    }
    private View text(View root,String label){if(root instanceof TextView&&label.contentEquals(((TextView)root).getText()))return root;if(root instanceof ViewGroup){ViewGroup group=(ViewGroup)root;for(int i=0;i<group.getChildCount();i++){View found=text(group.getChildAt(i),label);if(found!=null)return found;}}return null;}
    private View description(View root,String label){if(label.contentEquals(root.getContentDescription()==null?"":root.getContentDescription()))return root;if(root instanceof ViewGroup){ViewGroup group=(ViewGroup)root;for(int i=0;i<group.getChildCount();i++){View found=description(group.getChildAt(i),label);if(found!=null)return found;}}return null;}
    @Test public void navigationAndPrimaryControlsFitPortraitAndLandscape() throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(new Intent(context,MainActivity.class))){
            scenario.onActivity(a->{a.setShowWhenLocked(true);a.setTurnScreenOn(true);a.getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);});
            for(String module:new String[]{"player","library","dj","devices"}){
                scenario.onActivity(a->{View nav=a.getWindow().getDecorView().findViewWithTag("nav-"+module);assertNotNull(nav);nav.performClick();});
                InstrumentationRegistry.getInstrumentation().waitForIdleSync();Thread.sleep(200);
                device.takeScreenshot(new File(context.getExternalFilesDir(null),"clean-"+module+".png"));
                scenario.onActivity(a->{View root=a.getWindow().getDecorView();Rect header=new Rect();assertTrue("Header hidden in "+module,description(root,"Audio output and devices").getGlobalVisibleRect(header));assertTrue("Header moved offscreen: "+header,header.top>=0);for(String name:new String[]{"player","library","dj","devices"}){View nav=root.findViewWithTag("nav-"+name);Rect bounds=new Rect();assertTrue(name,nav.getGlobalVisibleRect(bounds));assertTrue(name+" "+bounds+" / "+root.getWidth(),bounds.left>=0&&bounds.right<=root.getWidth());assertTrue(name+" height "+bounds.height(),bounds.height()+1>=48*a.getResources().getDisplayMetrics().density);assertEquals(name.equals(module),nav.isSelected());}});
                device.takeScreenshot(new File(context.getExternalFilesDir(null),"clean-"+module+".png"));
            }
            scenario.onActivity(a->{View root=a.getWindow().getDecorView();assertNotNull(root.findViewWithTag("nav-cast"));View remote=root.findViewWithTag("nav-remote");assertNotNull(remote);remote.performClick();});
            scenario.onActivity(a->{View root=a.getWindow().getDecorView();assertTrue(root.findViewWithTag("nav-devices").isSelected());assertTrue(root.findViewWithTag("nav-remote").isSelected());assertNotNull(description(root,"Discover AMP Verse devices"));});
            device.takeScreenshot(new File(context.getExternalFilesDir(null),"clean-remote.png"));
            scenario.onActivity(a->{a.getWindow().getDecorView().findViewWithTag("nav-player").performClick();a.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE);});
            Thread.sleep(800);InstrumentationRegistry.getInstrumentation().waitForIdleSync();
            scenario.onActivity(a->{View root=a.getWindow().getDecorView();for(String name:new String[]{"player","library","dj","devices"}){View tab=root.findViewWithTag("nav-"+name);Rect bounds=new Rect();assertTrue(tab.getGlobalVisibleRect(bounds));assertTrue(bounds.right<=root.getWidth());}View play=description(root,"Play or pause");assertNotNull(play);Rect visible=new Rect();assertTrue(play.getGlobalVisibleRect(visible));assertTrue("Play target clipped",visible.height()+1>=48*a.getResources().getDisplayMetrics().density);});
            device.takeScreenshot(new File(context.getExternalFilesDir(null),"clean-player-landscape.png"));
            scenario.onActivity(a->a.getWindow().getDecorView().findViewWithTag("nav-dj").performClick());InstrumentationRegistry.getInstrumentation().waitForIdleSync();scenario.onActivity(a->scrolling(a.getWindow().getDecorView()).scrollTo(0,0));
            scenario.onActivity(a->{View root=a.getWindow().getDecorView();Rect left=new Rect(),right=new Rect(),mixer=new Rect();assertTrue(text(root,"DECK A").getGlobalVisibleRect(left));assertTrue(text(root,"DECK B").getGlobalVisibleRect(right));assertTrue(description(root,"DJ crossfader").getGlobalVisibleRect(mixer));assertTrue("Mixer not between decks",left.left<mixer.left&&mixer.right<=right.left);for(int deck=0;deck<2;deck++){View transport=root.findViewWithTag("deck-play-"+deck);Rect bounds=new Rect();assertTrue(transport.getGlobalVisibleRect(bounds));assertEquals("Clipped DJ transport "+deck,transport.getHeight(),bounds.height());}});
            device.takeScreenshot(new File(context.getExternalFilesDir(null),"clean-dj-landscape.png"));
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
