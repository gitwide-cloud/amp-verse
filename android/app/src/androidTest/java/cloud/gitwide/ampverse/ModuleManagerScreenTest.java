package cloud.gitwide.ampverse;
import android.content.*;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.uiautomator.*;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public final class ModuleManagerScreenTest {
 @Test public void discoverInstalledAndUpdatesAreReachableWithoutInterruptingPlayback()throws Exception{
  Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();UiDevice d=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
  c.startActivity(new Intent(c,MainActivity.class).putExtra("module","AMP Player").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_CLEAR_TOP|Intent.FLAG_ACTIVITY_SINGLE_TOP));
  assertTrue(d.wait(Until.hasObject(By.desc("AMP Verse module dropdown")),10000));d.findObject(By.desc("AMP Verse module dropdown")).click();
  assertTrue(d.wait(Until.hasObject(By.text("Add-ons")),3000));d.findObject(By.text("Add-ons")).click();
  for(String name:new String[]{"Discover","Installed","Updates"})assertTrue(d.wait(Until.hasObject(By.text(name)),5000));
  d.findObject(By.text("Installed")).click();assertTrue(d.wait(Until.hasObject(By.textContains("AMP DJ")),3000));
  d.findObject(By.text("Updates")).click();assertTrue(d.wait(Until.hasObject(By.textContains("No compatible companion updates")),3000));
  UiObject2 done=d.findObject(By.text(java.util.regex.Pattern.compile("Done",java.util.regex.Pattern.CASE_INSENSITIVE)));assertNotNull(done);done.click();
 }
}
