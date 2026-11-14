package cloud.gitwide.ampverse;
import android.content.*;
import android.graphics.Bitmap;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.uiautomator.*;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.io.*;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public final class ModuleScreenTest {
 @Test public void eachSignedCompanionConnectsAndRendersItsRealScreen()throws Exception{
  Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();UiDevice device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
  String[] ids={"dj","cast","podcasts","remote","lab"};String[] labels={"Choose playlist","Find LAN stations","Browse categories","Discover devices","Download MP3"};
  File captures=new File(c.getFilesDir(),"module-test-screenshots");captures.mkdirs();
  for(int i=0;i<ids.length;i++){
   c.startActivity(new Intent().setClassName("cloud.gitwide.ampverse."+ids[i],"cloud.gitwide.ampverse.CompanionActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("amp-test-keep-awake",true));
   boolean visible=device.wait(Until.hasObject(By.textContains(labels[i])),15000);
   if(!visible){Bitmap failed=InstrumentationRegistry.getInstrumentation().getUiAutomation().takeScreenshot();if(failed!=null){try(FileOutputStream out=new FileOutputStream(new File(captures,ids[i]+"-failed.png"))){failed.compress(Bitmap.CompressFormat.PNG,100,out);}failed.recycle();}}
   assertTrue(ids[i]+" must render its functional screen",visible);
   if(ids[i].equals("dj")){
    int old=c.getSharedPreferences("dj",0).getInt("fade-seconds",3),expected=old==30?29:old+1;
    try{device.findObject(By.text(old==30?"Fade −1s":"Fade +1s")).click();
     assertTrue("DJ must receive a real engine state, not just draw buttons",device.wait(Until.hasObject(By.textContains("Fade "+expected+"s")),10000));
    }finally{try{ModuleRpc.call(c,"fade",new org.json.JSONObject().put("seconds",old));}finally{c.getSharedPreferences("dj",0).edit().putInt("fade-seconds",old).commit();}}
   }
   Bitmap shot=InstrumentationRegistry.getInstrumentation().getUiAutomation().takeScreenshot();assertNotNull(shot);try(FileOutputStream out=new FileOutputStream(new File(captures,ids[i]+".png"))){shot.compress(Bitmap.CompressFormat.PNG,100,out);}shot.recycle();
  }
 }
}
