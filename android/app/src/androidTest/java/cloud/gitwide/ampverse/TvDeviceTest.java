package cloud.gitwide.ampverse;

import android.content.*;
import android.content.pm.*;
import android.view.*;
import android.widget.TextView;
import androidx.lifecycle.Lifecycle;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.uiautomator.*;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.io.File;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public final class TvDeviceTest {
    @Test public void tvLauncherAndTouchRequirementsAreDeclared()throws Exception {
        assertEquals(.96,SharedDjLogic.tempoRate(120,1,125,.16),.0001);
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        Intent launcher=new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LEANBACK_LAUNCHER).setPackage(context.getPackageName());
        assertTrue(context.getPackageManager().queryIntentActivities(launcher,0).stream().anyMatch(info->info.activityInfo.name.equals(TvActivity.class.getName())));
        PackageInfo info=context.getPackageManager().getPackageInfo(context.getPackageName(),PackageManager.GET_CONFIGURATIONS);
        for(FeatureInfo feature:info.reqFeatures)if("android.hardware.touchscreen".equals(feature.name)||"android.hardware.faketouch".equals(feature.name))assertEquals(0,feature.flags&FeatureInfo.FLAG_REQUIRED);
    }
    @Test public void normalAppEntryOpensTelevisionLayout(){
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        if(!TvActivity.television(context))return;
        UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
        context.startActivity(new Intent(context,MainActivity.class).putExtra("module","AMP Library").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_CLEAR_TOP));
        assertTrue(device.wait(Until.hasObject(By.textContains("TV ·")),10000));
        assertTrue(device.wait(Until.hasObject(By.desc("Search saved music")),5000));
    }
    @Test public void dpadNavigatesMainWorkflowsAndBackgroundingKeepsPlaybackOwner()throws Exception {
        Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
        UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
        try(ActivityScenario<TvActivity> scenario=ActivityScenario.launch(TvActivity.class)) {
            assertTrue(device.wait(Until.hasObject(By.textContains("TV ·")),10000));
            String[] pages={"Library","Playlists","Podcasts","Devices","Settings","Player"};
            for(String page:pages){
                scenario.onActivity(activity->{View tab=activity.findViewById(android.R.id.content).findViewWithTag("tv-nav-"+page);assertNotNull(tab);tab.requestFocus();assertTrue(tab.hasFocus());});
                device.pressDPadCenter();device.waitForIdle();
                scenario.onActivity(activity->{TextView title=activity.findViewById(android.R.id.content).findViewWithTag("tv-heading");assertEquals(page,title.getText().toString());});
                device.pressDPadRight();device.waitForIdle();
                scenario.onActivity(activity->{assertNotNull(activity.getCurrentFocus());assertTrue(activity.getCurrentFocus().isShown());});
                if(!page.equals("Player")){device.pressBack();device.waitForIdle();scenario.onActivity(activity->{TextView title=activity.findViewById(android.R.id.content).findViewWithTag("tv-heading");assertEquals("Player",title.getText().toString());});}
            }
            final String[] owner={""};scenario.onActivity(activity->owner[0]=PlaybackService.owner());
            scenario.moveToState(Lifecycle.State.CREATED);scenario.moveToState(Lifecycle.State.RESUMED);device.waitForIdle();
            scenario.onActivity(activity->assertEquals(owner[0],PlaybackService.owner()));
            File folder=new File(context.getExternalFilesDir(null),"tv-tests");folder.mkdirs();assertTrue(device.takeScreenshot(new File(folder,"tv-player.png")));
            scenario.onActivity(activity->activity.findViewById(android.R.id.content).findViewWithTag("tv-nav-Library").requestFocus());device.pressDPadCenter();device.waitForIdle();assertTrue(device.takeScreenshot(new File(folder,"tv-library.png")));
        }
    }
}
