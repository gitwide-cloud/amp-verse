package cloud.gitwide.ampverse;
import android.content.Intent;
import android.view.View;
import android.view.ViewGroup;
import android.widget.TextView;
import androidx.test.core.app.ActivityScenario;
import androidx.test.core.app.ApplicationProvider;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;
/** No network or playback: disconnected paths stay discoverable, but cannot send commands. */
@RunWith(AndroidJUnit4.class)
public class CastPathsDeviceTest {
    private TextView find(View view,String text){
        if(view instanceof TextView && text.contentEquals(((TextView)view).getText()))return (TextView)view;
        if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++){TextView result=find(group.getChildAt(i),text);if(result!=null)return result;}}
        return null;
    }
    @Test public void disconnectedPathsStayVisibleAndConnectionDetailsStayAdvanced(){
        org.junit.Assume.assumeTrue("Full Cast UI only",BuildConfig.EDITION.equals("full"));
        Intent intent=new Intent(ApplicationProvider.getApplicationContext(),MainActivity.class).putExtra("module","AMP Cast").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(intent)){
            scenario.onActivity(activity->{
                View root=activity.getWindow().getDecorView();
                for(String text:new String[]{"Local listening · host → this phone","Google Cast · host → Google speaker","Server speakers · host → its audio output","Station source · change the host’s program","Remote desktop broadcast · desktop master → host"}){
                    TextView path=find(root,text);assertNotNull(text,path);assertTrue(text,path.isShown());
                }
                for(String text:new String[]{"Listen to host on this phone","Send host stream to Chromecast","Request playback on server speakers","Start host station","Request desktop host Player/DJ master…"}){
                    TextView action=find(root,text);assertNotNull(text,action);assertFalse(text,action.isEnabled());
                }
                TextView connect=find(root,"Connect");assertNotNull(connect);assertFalse(connect.isShown());
                TextView advanced=find(root,"Advanced connection settings");assertNotNull(advanced);advanced.performClick();assertTrue(connect.isShown());
            });
        }
    }
}
