// Derived from MP3 Lab (MIT). Copyright (c) 2026 MP3 Lab contributors.
package cloud.gitwide.ampverse.lab;

import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import com.arthenica.ffmpegkit.*;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.io.File;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public class AudioEngineTest {
    private void run(String command) {
        FFmpegSession s=FFmpegKit.execute(command);
        assertTrue(s.getAllLogsAsString(), ReturnCode.isSuccess(s.getReturnCode()));
    }
    @Test public void trimFadesGainNormalizeAndMp3() throws Exception {
        File dir=InstrumentationRegistry.getInstrumentation().getTargetContext().getCacheDir();
        File source=new File(dir,"test-source.wav"), output=new File(dir,"test-output.mp3");
        run("-y -f lavfi -i sine=frequency=440:duration=8 -c:a pcm_s16le "+source);
        // Exercise the actual UI command builder, including normalize and manual gain.
        MainActivity activity=(MainActivity)InstrumentationRegistry.getInstrumentation().startActivitySync(
            new android.content.Intent(InstrumentationRegistry.getInstrumentation().getTargetContext(), MainActivity.class)
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK));
        try {
            for (String field:new String[]{"start","end","fadeIn","fadeOut","gainDb"}) {
                java.lang.reflect.Field f=MainActivity.class.getDeclaredField(field);f.setAccessible(true);
                f.setDouble(activity, field.equals("start")?2:field.equals("end")?6:field.equals("gainDb")?-3:0.5);
            }
            java.lang.reflect.Field in=MainActivity.class.getDeclaredField("input");in.setAccessible(true);in.set(activity,source);
            activity.duration=8;activity.waveform=activity.readWaveform(source,8);
            assertTrue(activity.waveform.length>=1600);
            InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{
                activity.filename.setText("Skin-check.wav");activity.refresh();activity.status.setText("Ready. Audio engine verified.");
            });
            java.lang.reflect.Field norm=MainActivity.class.getDeclaredField("normalize");norm.setAccessible(true);
            InstrumentationRegistry.getInstrumentation().runOnMainSync(()->((android.widget.CheckBox)get(norm,activity)).setChecked(true));
            java.lang.reflect.Method cmd=MainActivity.class.getDeclaredMethod("command",File.class,boolean.class);cmd.setAccessible(true);
            run((String)cmd.invoke(activity,output,true));
            MediaInformation info=FFprobeKit.getMediaInformation(output.toString()).getMediaInformation();
            assertNotNull(info);
            assertEquals(4,Double.parseDouble(info.getDuration()),0.15);
            assertEquals("mp3",info.getStreams().get(0).getCodec());
            assertEquals(256000,Long.parseLong(info.getStreams().get(0).getBitrate()));
            File pcm=new File(dir,"test-output.pcm");
            run("-y -i "+output+" -ac 1 -ar 48000 -f s16le "+pcm);
            byte[] samples=java.nio.file.Files.readAllBytes(pcm.toPath());
            double middle=rms(samples,1.5,2.5);
            assertTrue("Normalized signal with -3 dB gain: "+middle,middle>0.06&&middle<0.18);
            assertTrue("Fade in must attenuate opening",rms(samples,0,0.1)<middle*0.4);
            assertTrue("Fade out must attenuate ending",rms(samples,3.9,4)<middle*0.4);
            pcm.delete();
            InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{
                android.graphics.Bitmap bitmap=android.graphics.Bitmap.createBitmap(activity.root.getWidth(),activity.root.getHeight(),android.graphics.Bitmap.Config.ARGB_8888);
                activity.root.draw(new android.graphics.Canvas(bitmap));
                String outputDir=InstrumentationRegistry.getArguments().getString("additionalTestOutputDir",activity.getExternalFilesDir(null).toString());
                File imageDir=new File(outputDir);imageDir.mkdirs();
                try(java.io.FileOutputStream image=new java.io.FileOutputStream(new File(imageDir,"android-skin.png"))){bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,image);}catch(Exception e){throw new RuntimeException(e);}
                bitmap.recycle();
            });
            android.media.MediaPlayer player=new android.media.MediaPlayer();
            player.setDataSource(output.toString());player.prepare();player.start();
            assertTrue(player.isPlaying());player.stop();player.release();
            run("-y -i "+output+" -af astats -f null -");
            // Exercise the real Android document output path, not just the encoder's cache file.
            android.content.ContentValues values=new android.content.ContentValues();
            values.put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME,"mp3-lab-export-test.mp3");
            values.put(android.provider.MediaStore.MediaColumns.MIME_TYPE,"audio/mpeg");
            android.net.Uri saved=activity.getContentResolver().insert(android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI,values);
            assertNotNull(saved);
            try{
                try(java.io.OutputStream stale=activity.getContentResolver().openOutputStream(saved,"w")){stale.write(new byte[300000]);}
                InstrumentationRegistry.getInstrumentation().runOnMainSync(()->activity.export(saved));
                long deadline=android.os.SystemClock.uptimeMillis()+30000;
                while(activity.busy&&android.os.SystemClock.uptimeMillis()<deadline)Thread.sleep(50);
                assertFalse("Export timed out",activity.busy);
                InstrumentationRegistry.getInstrumentation().waitForIdleSync();
                InstrumentationRegistry.getInstrumentation().runOnMainSync(()->assertEquals("Saved and verified 256 kbps MP3",activity.status.getText().toString()));
                android.media.MediaPlayer savedPlayer=new android.media.MediaPlayer();
                try{savedPlayer.setDataSource(activity,saved);savedPlayer.prepare();assertEquals(4000,savedPlayer.getDuration(),150);savedPlayer.start();assertTrue(savedPlayer.isPlaying());}finally{savedPlayer.release();}
                // Tap seeking must change the real player position, including a nonzero clip offset.
                InstrumentationRegistry.getInstrumentation().runOnMainSync(()->activity.preview(true));
                deadline=android.os.SystemClock.uptimeMillis()+30000;
                while(activity.busy&&android.os.SystemClock.uptimeMillis()<deadline)Thread.sleep(50);
                InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{
                    assertNotNull(activity.player);activity.player.pause();assertNotNull("Seek mode must be visible",activity.seekTarget);activity.seekTarget.performClick();assertFalse(activity.waveformView.placing);activity.waveformView.fit();
                    Retro.Wave w=activity.waveformView;float x=w.getWidth()*4.0f/8,y=w.getHeight()*.4f;long now=android.os.SystemClock.uptimeMillis();
                    android.view.MotionEvent down=android.view.MotionEvent.obtain(now,now,0,x,y,0),up=android.view.MotionEvent.obtain(now,now+20,1,x,y,0);
                    w.onTouchEvent(down);w.onTouchEvent(up);down.recycle();up.recycle();
                    assertEquals(2,activity.start,0);assertEquals(6,activity.end,0);
                });
                Thread.sleep(250);
                InstrumentationRegistry.getInstrumentation().runOnMainSync(()->{
                    assertEquals("Actual player seeks relative to the rendered clip",2000,activity.player.getCurrentPosition(),150);
                    assertEquals(4,activity.waveformView.position,.15);activity.stop();
                });
            }finally{activity.getContentResolver().delete(saved,null,null);}

        } finally {
            InstrumentationRegistry.getInstrumentation().runOnMainSync(activity::finish);
            source.delete();output.delete();
        }
    }
    private Object get(java.lang.reflect.Field f,Object o){try{return f.get(o);}catch(Exception e){throw new RuntimeException(e);}}
    private double rms(byte[] b,double from,double to){
        int start=(int)(from*48000),end=Math.min(b.length/2,(int)(to*48000));double sum=0;
        for(int i=start;i<end;i++){double v=(short)((b[2*i]&255)|(b[2*i+1]<<8))/32768.0;sum+=v*v;}
        return Math.sqrt(sum/(end-start));
    }
}
