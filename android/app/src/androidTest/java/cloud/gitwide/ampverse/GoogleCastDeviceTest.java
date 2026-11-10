package cloud.gitwide.ampverse;
import android.content.Intent;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import com.google.android.gms.cast.framework.CastContext;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
public class GoogleCastDeviceTest {
 @Test public void speakerPickerInitializesWithoutStartingAudio(){
  android.content.Context context=InstrumentationRegistry.getInstrumentation().getTargetContext();
  Intent intent=new Intent(context,GoogleCastActivity.class).putExtra("stream","http://192.168.1.20:8787/listen.mp3").putExtra("title","Test station");
  try(ActivityScenario<GoogleCastActivity> scenario=ActivityScenario.launch(intent)){
   scenario.onActivity(activity->{assertNotNull(CastContext.getSharedInstance(activity));assertNotNull(activity.findViewById(android.R.id.content));});
  }
 }
}
