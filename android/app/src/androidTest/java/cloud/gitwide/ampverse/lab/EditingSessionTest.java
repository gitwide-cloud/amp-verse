// Derived from MP3 Lab (MIT). Copyright (c) 2026 MP3 Lab contributors.
package cloud.gitwide.ampverse.lab;

import android.content.pm.ActivityInfo;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.InputDevice;
import android.view.WindowManager;
import androidx.test.core.app.ActivityScenario;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.lifecycle.Lifecycle;
import com.arthenica.ffmpegkit.*;
import java.io.*;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.Test;
import org.junit.Before;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public class EditingSessionTest {
    @Before public void wakeTestDevice()throws Exception{androidx.test.uiautomator.UiDevice.getInstance(InstrumentationRegistry.getInstrumentation()).wakeUp();}
    File fixture() {
        File dir=new File(InstrumentationRegistry.getInstrumentation().getTargetContext().getFilesDir(),"amp-lab-editing");dir.mkdirs();
        new android.util.AtomicFile(new File(dir,"session.json")).delete();
        File f=new File(dir,"session-test.wav");
        assertTrue(ReturnCode.isSuccess(FFmpegKit.execute("-y -f lavfi -i sine=frequency=440:duration=12 -c:a pcm_s16le "+f).getReturnCode()));return f;
    }
    void bind(ActivityScenario<MainActivity> scenario,File file)throws Exception {
        AtomicReference<MainActivity> ref=new AtomicReference<>();scenario.onActivity(ref::set);
        float[] wave=ref.get().readWaveform(file,12);
        scenario.onActivity(a->{a.input=file;a.name="session-test.wav";a.filename.setText(a.name);a.duration=12;
            a.start=2.345;a.end=10.678;a.fadeIn=.456;a.fadeOut=.789;a.gainDb=-4.2;a.normalize.setChecked(true);a.editedPreview=false;a.waveform=wave;
            a.refresh();a.waveformView.restoreViewport(8,2.2,true);SessionStore.writeWave(a);assertTrue(SessionStore.save(a));
        });
    }
    void assertSession(MainActivity a){
        assertNotNull(a.input);assertTrue(a.input.isFile());assertEquals("session-test.wav",a.name);assertEquals(12,a.duration,0);
        assertEquals(2.345,a.start,1e-6);assertEquals(10.678,a.end,1e-6);assertEquals(.456,a.fadeIn,1e-6);assertEquals(.789,a.fadeOut,1e-6);
        assertEquals(-4.2,a.gainDb,1e-6);assertTrue(a.normalize.isChecked());assertFalse(a.editedPreview);
        assertEquals(8,a.waveformView.zoom,1e-6);assertEquals(2.2,a.waveformView.viewStart,1e-6);assertTrue(a.waveform.length>=2400);
        assertTrue((a.getWindow().getAttributes().flags&WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)!=0);
    }
    void cleanup(File source){
        File dir=source.getParentFile();new android.util.AtomicFile(new File(dir,"session.json")).delete();source.delete();new File(dir,source.getName()+".wave").delete();
    }
    @Test public void rotateRecreateAndBackgroundRestoreRetainEdits()throws Exception{
        File source=fixture();
        try{
            try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(MainActivity.class)){
                bind(scenario,source);AtomicReference<MainActivity> original=new AtomicReference<>();scenario.onActivity(a->{original.set(a);a.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE);});
                InstrumentationRegistry.getInstrumentation().waitForIdleSync();
                scenario.onActivity(a->{assertSame("Rotation must retain ongoing session",original.get(),a);assertSession(a);a.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT);});
                InstrumentationRegistry.getInstrumentation().waitForIdleSync();
                wakeTestDevice();
                // Observe recreation even if the device's keyguard leaves the new Activity stopped.
                // Session restoration must work independently of whether Android resumes its window.
                android.app.Instrumentation instrumentation=InstrumentationRegistry.getInstrumentation();
                android.app.Instrumentation.ActivityMonitor monitor=instrumentation.addMonitor(MainActivity.class.getName(),null,false);
                try{
                    scenario.onActivity(MainActivity::recreate);
                    MainActivity recreated=(MainActivity)instrumentation.waitForMonitorWithTimeout(monitor,10000);
                    assertNotNull("Activity must be recreated",recreated);assertNotSame(original.get(),recreated);instrumentation.waitForIdleSync();
                    instrumentation.runOnMainSync(()->assertSession(recreated));
                }finally{instrumentation.removeMonitor(monitor);}
                scenario.moveToState(Lifecycle.State.CREATED);
            }
            // A new Activity with no instance-state Bundle must recover the app-private disk checkpoint.
            wakeTestDevice();
            try(ActivityScenario<MainActivity> reopened=ActivityScenario.launch(MainActivity.class)){
                reopened.onActivity(a->{assertSession(a);a.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED);});
            }
        }finally{cleanup(source);}
    }
    void touch(Retro.Wave w,long down,long time,int action,float x,float y){MotionEvent e=MotionEvent.obtain(down,time,action,x,y,0);w.onTouchEvent(e);e.recycle();}
    @Test public void zoomPanTapAndMillisecondNudgeUseVisibleTimes()throws Exception{
        File source=fixture();
        try(ActivityScenario<MainActivity> scenario=ActivityScenario.launch(MainActivity.class)){
            bind(scenario,source);
            scenario.onActivity(a->{Retro.Wave w=a.waveformView;a.start=1;a.end=11;a.refresh();w.restoreViewport(4,3,true);w.placing=true;
                float width=w.getWidth(),y=w.getHeight()*.4f;assertTrue(width>0);
                long down=SystemClock.uptimeMillis();touch(w,down,down,MotionEvent.ACTION_DOWN,width*.5f,y);touch(w,down,down+30,MotionEvent.ACTION_UP,width*.5f,y);
                assertEquals("IN uses zoomed viewport, not whole-track coordinates",4.5,a.start,1e-4);
                w.nudge(.001);assertEquals(4.501,a.start,1e-4);
                a.start=1;a.end=11;a.refresh();w.restoreViewport(4,3,false);
                down+=100;touch(w,down,down,MotionEvent.ACTION_DOWN,width*.7f,y);touch(w,down,down+30,MotionEvent.ACTION_MOVE,width*.3f,y);touch(w,down,down+60,MotionEvent.ACTION_UP,width*.3f,y);
                assertEquals(4.2,w.viewStart,1e-4);assertEquals(1,a.start,0);assertEquals(11,a.end,0);
                // Pinch-out events must increase zoom while leaving both trim points unchanged.
                double previousZoom=w.zoom;pinch(w,width,y);assertTrue("Pinch zoom must work",w.zoom>previousZoom);assertEquals(1,a.start,0);assertEquals(11,a.end,0);
                w.focusMarker(false);w.zoomBy(2);assertTrue(w.viewStart<=a.end&&w.visibleEnd()>=a.end);
                w.focusMarker(true);a.root.jumpDrawablesToCurrentState();
                String outputDir=InstrumentationRegistry.getArguments().getString("additionalTestOutputDir",a.getExternalFilesDir(null).toString());
                android.graphics.Bitmap bitmap=android.graphics.Bitmap.createBitmap(a.root.getWidth(),a.root.getHeight(),android.graphics.Bitmap.Config.ARGB_8888);a.root.draw(new android.graphics.Canvas(bitmap));
                try(FileOutputStream out=new FileOutputStream(new File(outputDir,"android-zoom.png"))){bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,out);}catch(IOException e){throw new RuntimeException(e);}bitmap.recycle();
                w.fit();assertEquals(1,w.zoom,0);assertEquals(0,w.viewStart,0);
            });
        }finally{cleanup(source);}
    }
    void pinch(Retro.Wave w,float width,float y){
        long down=SystemClock.uptimeMillis();touch(w,down,down,MotionEvent.ACTION_DOWN,width*.35f,y);
        MotionEvent.PointerProperties[] properties={new MotionEvent.PointerProperties(),new MotionEvent.PointerProperties()};
        MotionEvent.PointerCoords[] coords={new MotionEvent.PointerCoords(),new MotionEvent.PointerCoords()};
        for(int i=0;i<2;i++){properties[i].id=i;properties[i].toolType=MotionEvent.TOOL_TYPE_FINGER;coords[i].pressure=1;coords[i].size=1;coords[i].y=y;}
        for(int step=0;step<8;step++){
            coords[0].x=width*(.35f-step*.035f);coords[1].x=width*(.65f+step*.035f);
            int action=step==0?MotionEvent.ACTION_POINTER_DOWN|(1<<MotionEvent.ACTION_POINTER_INDEX_SHIFT):MotionEvent.ACTION_MOVE;
            MotionEvent event=MotionEvent.obtain(down,down+20+step*20,action,2,properties,coords,0,0,1,1,0,0,InputDevice.SOURCE_TOUCHSCREEN,0);w.onTouchEvent(event);event.recycle();
        }
        MotionEvent up=MotionEvent.obtain(down,down+200,MotionEvent.ACTION_POINTER_UP|(1<<MotionEvent.ACTION_POINTER_INDEX_SHIFT),2,properties,coords,0,0,1,1,0,0,InputDevice.SOURCE_TOUCHSCREEN,0);w.onTouchEvent(up);up.recycle();
        touch(w,down,down+220,MotionEvent.ACTION_UP,coords[0].x,y);
    }
}
