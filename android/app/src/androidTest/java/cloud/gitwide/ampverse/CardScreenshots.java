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
            String result=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation()).executeShellCommand("mkdir -p /sdcard/Download/amp-verse-card-ui && screencap -p "+path+" && test -s "+path+" && echo CAPTURED");
            assertTrue("Actual screen capture failed: "+name,result.contains("CAPTURED"));
        }catch(java.io.IOException error){throw new AssertionError("Could not collect actual screen: "+name,error);}
    }
}
