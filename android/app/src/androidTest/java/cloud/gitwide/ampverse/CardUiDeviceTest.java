package cloud.gitwide.ampverse;

import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.TextView;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;

/** Real Activity hierarchy/actions; does not start music, change sources, or enable sync. */
@RunWith(AndroidJUnit4.class)
public final class CardUiDeviceTest {
    private static View tagged(View root,String tag){if(tag.equals(root.getTag()))return root;if(root instanceof ViewGroup){ViewGroup group=(ViewGroup)root;for(int i=0;i<group.getChildCount();i++){View found=tagged(group.getChildAt(i),tag);if(found!=null)return found;}}return null;}
    private static View text(View root,String text){if(root instanceof TextView&&text.contentEquals(((TextView)root).getText()))return root;if(root instanceof ViewGroup){ViewGroup group=(ViewGroup)root;for(int i=0;i<group.getChildCount();i++){View found=text(group.getChildAt(i),text);if(found!=null)return found;}}return null;}
    private static void capture(String name){CardScreenshots.capture(name);}
    private static void awaitService(ActivityScenario<MainActivity> screen){java.util.concurrent.atomic.AtomicBoolean ready=new java.util.concurrent.atomic.AtomicBoolean();for(int n=0;n<100;n++){screen.onActivity(a->ready.set(PlaybackService.dj()!=null));if(ready.get()){androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().waitForIdleSync();return;}android.os.SystemClock.sleep(50);}fail("Playback service did not connect for card navigation test");}
    @Test public void libraryCardsKeepLocalAndServerActionsDistinctWithoutChangingPlayback(){
        try(ActivityScenario<MainActivity> screen=ActivityScenario.launch(MainActivity.class)){
            awaitService(screen);
            screen.onActivity(activity->{String owner=PlaybackService.owner();View root=activity.getWindow().getDecorView();View library=tagged(root,"nav-library");assertNotNull(library);library.performClick();root=activity.getWindow().getDecorView();assertNotNull(text(root,"Add music\nImport songs or folders"));View playlists=text(root,"Playlists");assertNotNull(playlists);playlists.performClick();root=activity.getWindow().getDecorView();assertNotNull(text(root,"Server playlist sync"));assertNotNull(text(root,"+ Server playlist"));assertNotNull(text(root,"+ Playlist"));assertEquals(owner,PlaybackService.owner());for(String name:new String[]{"player","library","dj","devices"}){View tab=tagged(root,"nav-"+name);assertTrue(tab instanceof Button);assertTrue(tab.getMinimumHeight()>=48*activity.getResources().getDisplayMetrics().density-1);assertNotNull(tab.getContentDescription());}});
            androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().waitForIdleSync();capture("card-library-playlists.png");
        }
    }
    @Test public void focusedDjDeckKeepsSecondarySelectionAndPinnedCrossfader(){
        org.junit.Assume.assumeTrue(BuildConfig.EDITION.equals("full"));
        try(ActivityScenario<MainActivity> screen=ActivityScenario.launch(MainActivity.class)){
            awaitService(screen);
            screen.onActivity(activity->{String owner=PlaybackService.owner();View dj=tagged(activity.getWindow().getDecorView(),"nav-dj");assertNotNull(dj);dj.performClick();assertEquals(owner,PlaybackService.owner());});
            androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().waitForIdleSync();
            screen.onActivity(activity->{View root=activity.getWindow().getDecorView();assertNotNull(PlaybackService.dj());assertNotNull(tagged(root,"deck-select-a"));assertNotNull(tagged(root,"deck-select-b"));assertNotNull(text(root,"Seek & staged"));assertNotNull(text(root,"Deck settings"));assertNotNull(tagged(root,"deck-play-"+PlaybackService.dj().destination));});
            androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().waitForIdleSync();capture("card-dj-focused.png");
        }
    }
    @Test public void devicesCardsKeepDiscoveryAndRemoteSeparateWithoutTakingPlayback(){
        org.junit.Assume.assumeTrue(BuildConfig.EDITION.equals("full"));
        try(ActivityScenario<MainActivity> screen=ActivityScenario.launch(MainActivity.class)){
            awaitService(screen);
            screen.onActivity(activity->{String owner=PlaybackService.owner();View root=activity.getWindow().getDecorView();tagged(root,"nav-devices").performClick();root=activity.getWindow().getDecorView();tagged(root,"nav-cast").performClick();root=activity.getWindow().getDecorView();for(String title:new String[]{"Station · listening & control","NOW PLAYING","Server","Output","Stations","Discover","Quick Actions","Nearby AMP servers","Offline · playlists & music","Saved Playlists · sync & manage"})assertNotNull(title,text(root,title));assertEquals(owner,PlaybackService.owner());text(root,"Remote · control your desktop").performClick();root=activity.getWindow().getDecorView();assertNotNull(text(root,"Remote · your desktop"));assertNotNull(tagged(root,"nav-devices"));assertEquals(owner,PlaybackService.owner());});
            androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().waitForIdleSync();capture("card-devices-remote.png");
        }
    }

}
