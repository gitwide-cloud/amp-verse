package cloud.gitwide.ampverse;

import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.uiautomator.UiDevice;
import static org.junit.Assert.assertTrue;

/** Test-only shell capture survives Gradle uninstalling the debug app after the run. */
final class CardScreenshots {
    private CardScreenshots(){}
    static void capture(String name){
        if(!name.matches("[a-z0-9-]+\\.png"))throw new IllegalArgumentException("Invalid screenshot fixture name");
        try{
            String path="/sdcard/Download/amp-verse-card-ui/"+name;
            UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
            // UiAutomation executes an argv command, not a shell script. Keep
            // these separate so && is never passed literally to mkdir.
            device.executeShellCommand("mkdir -p /sdcard/Download/amp-verse-card-ui");
            String capture=device.executeShellCommand("screencap -p "+path);
            String size=device.executeShellCommand("stat -c %s "+path).trim();
            assertTrue("Actual screen capture failed: "+name+"; bytes="+size+"; "+capture,size.matches("[0-9]+")&&Long.parseLong(size)>8);
        }catch(java.io.IOException error){throw new AssertionError("Could not collect actual screen: "+name,error);}
    }
}
