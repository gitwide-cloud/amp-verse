package cloud.gitwide.ampverse;

import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.widget.CheckBox;
import android.widget.RadioButton;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.io.File;
import java.io.FileOutputStream;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public class SettingsDeviceTest {
    @Test public void settingsThemeAndPlaybackPersistAcrossRecreation() throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        SharedPreferences prefs=context.getSharedPreferences("settings",Context.MODE_PRIVATE);
        String original=prefs.getString("theme","classic");boolean repeat=prefs.getBoolean("repeat",false);
        InstrumentationRegistry.getInstrumentation().getUiAutomation().executeShellCommand("input keyevent 224").close();
        InstrumentationRegistry.getInstrumentation().getUiAutomation().executeShellCommand("wm dismiss-keyguard").close();
        try(ActivityScenario<SettingsActivity> scenario=ActivityScenario.launch(SettingsActivity.class)){
            scenario.onActivity(activity->{
                activity.getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
                activity.setShowWhenLocked(true);activity.setTurnScreenOn(true);
                RadioButton light=activity.getWindow().getDecorView().findViewWithTag("modern-light");assertNotNull(light);light.performClick();
                CheckBox queue=activity.getWindow().getDecorView().findViewWithTag("repeat");queue.setChecked(!repeat);
            });
            scenario.recreate();
            scenario.onActivity(activity->{
                RadioButton light=activity.getWindow().getDecorView().findViewWithTag("modern-light");assertTrue(light.isChecked());
                assertEquals("modern-light",prefs.getString("theme",""));
                assertEquals(!repeat,prefs.getBoolean("repeat",repeat));
                assertEquals(new Appearance(activity).background,new Appearance(context).background);
            });
            InstrumentationRegistry.getInstrumentation().waitForIdleSync();
            Bitmap screenshot=InstrumentationRegistry.getInstrumentation().getUiAutomation().takeScreenshot();
            if(screenshot!=null)try(FileOutputStream output=new FileOutputStream(new File(context.getFilesDir(),"settings-qa.png"))){screenshot.compress(Bitmap.CompressFormat.PNG,100,output);}
        }finally{prefs.edit().putString("theme",original).putBoolean("repeat",repeat).commit();}
    }
    @Test public void updateVersionResultsAreHonest() throws Exception {
        assertTrue(ReleaseCheck.describe("{\"tag_name\":\"v0.2.0\"}","0.1.1").startsWith("New release"));
        assertTrue(ReleaseCheck.describe("{\"tag_name\":\"v0.1.1\"}","0.1.1").startsWith("You have the latest"));
        assertTrue(ReleaseCheck.describe("{\"tag_name\":\"v0.1.0\"}","0.1.1").contains("newer than"));
        assertTrue(ReleaseCheck.compare("1.10.0","1.2.0")>0);
        try{ReleaseCheck.compare("v2.0.0-beta","1.0.0");fail("Unsupported tag accepted");}catch(IllegalArgumentException expected){}
    }
}
