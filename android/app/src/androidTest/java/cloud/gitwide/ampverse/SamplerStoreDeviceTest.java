package cloud.gitwide.ampverse;
import android.content.*;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.util.UUID;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
public final class SamplerStoreDeviceTest {
 @Test public void staleClipCannotReplaceANewerPadOrItsManualLoopSetting(){Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();String name="sampler-fixture-"+UUID.randomUUID();SharedPreferences prefs=c.getSharedPreferences(name,0);
  try{long empty=SamplerStore.revision(prefs,0,0);SamplerStore.save(prefs,0,0,"first.mp3","First",false);long loaded=SamplerStore.revision(prefs,0,0);assertTrue(loaded>empty);try{SamplerStore.expect(prefs,0,0,empty);fail();}catch(IllegalStateException expected){assertTrue(expected.getMessage().contains("changed"));}
   SamplerStore.save(prefs,0,0,"first.mp3","Renamed loop",true);try{SamplerStore.expect(prefs,0,0,loaded);fail();}catch(IllegalStateException expected){}assertEquals("first.mp3",prefs.getString("sample-0-0",""));assertTrue(prefs.getBoolean("loop-0-0",false));SamplerStore.clear(prefs,0,0);assertEquals("",prefs.getString("sample-0-0",""));assertTrue(SamplerStore.revision(prefs,0,0)>loaded);
  }finally{c.deleteSharedPreferences(name);}
 }
}
