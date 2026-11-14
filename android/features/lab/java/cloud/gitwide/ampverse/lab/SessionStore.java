// Derived from MP3 Lab (MIT). Copyright (c) 2026 MP3 Lab contributors.
package cloud.gitwide.ampverse.lab;

import cloud.gitwide.ampverse.R;

import android.util.AtomicFile;
import org.json.JSONObject;
import java.io.*;
import java.nio.charset.StandardCharsets;

/** App-private source audio and atomic editing checkpoints survive Activity/process recreation. */
final class SessionStore {
    static File directory(MainActivity a){File d=new File(a.getFilesDir(),"amp-lab-editing");if(!d.isDirectory()&&!d.mkdirs())throw new IllegalStateException("Cannot create session directory");return d;}
    static AtomicFile metadata(MainActivity a){return new AtomicFile(new File(directory(a),"session.json"));}
    static boolean owns(MainActivity a,File file)throws IOException{return file!=null&&directory(a).getCanonicalFile().equals(file.getCanonicalFile().getParentFile());}
    static JSONObject snapshot(MainActivity a)throws Exception{
        if(a.input==null||!a.input.isFile()||!owns(a,a.input))return null;
        JSONObject s=new JSONObject();s.put("version",1);s.put("file",a.input.getName());s.put("name",a.name);s.put("duration",a.duration);
        s.put("start",a.start);s.put("end",a.end);s.put("fadeIn",a.fadeIn);s.put("fadeOut",a.fadeOut);s.put("gain",a.gainDb);
        s.put("normalize",a.normalize.isChecked());s.put("edited",a.editedPreview);
        s.put("lastExportedEdit",a.lastExportedEdit);
        s.put("zoom",a.waveformView.zoom);s.put("viewStart",a.waveformView.viewStart);s.put("targetStart",a.waveformView.targetStart);
        s.put("scrollY",a.editorScroll==null?0:a.editorScroll.getScrollY());return s;
    }
    static boolean save(MainActivity a){
        FileOutputStream stream=null;AtomicFile atomic=null;
        try{JSONObject s=snapshot(a);if(s==null)return false;atomic=metadata(a);stream=atomic.startWrite();stream.write(s.toString().getBytes(StandardCharsets.UTF_8));atomic.finishWrite(stream);return true;}
        catch(Exception e){if(atomic!=null&&stream!=null)atomic.failWrite(stream);a.report("Session autosave failed: "+e.getMessage());return false;}
    }
    static boolean restore(MainActivity a,String saved){
        try{
            if(saved==null){try(InputStream in=metadata(a).openRead();ByteArrayOutputStream bytes=new ByteArrayOutputStream()){MainActivity.copy(in,bytes);saved=bytes.toString("UTF-8");}}
            JSONObject s=new JSONObject(saved);String basename=s.getString("file");
            if(!new File(basename).getName().equals(basename))return false;
            File file=new File(directory(a),basename);double duration=s.getDouble("duration");
            if(!file.isFile()||!owns(a,file)||!Double.isFinite(duration)||duration<=0)return false;
            a.input=file;a.name=s.optString("name","song");a.duration=duration;
            a.start=finite(s.optDouble("start",0),0);a.end=finite(s.optDouble("end",duration),duration);
            a.fadeIn=finite(s.optDouble("fadeIn",0),0);a.fadeOut=finite(s.optDouble("fadeOut",0),0);a.gainDb=finite(s.optDouble("gain",0),0);
            a.normalize.setChecked(s.optBoolean("normalize",false));a.editedPreview=s.optBoolean("edited",true);
            a.lastExportedEdit=s.optString("lastExportedEdit","");
            a.filename.setText(a.name);a.refresh();a.waveformView.restoreViewport(s.optDouble("zoom",1),s.optDouble("viewStart",0),s.optBoolean("targetStart",true));
            a.refreshZoom();a.gainBar.setProgress((int)((a.gainDb+24)/36*1000));
            int scrollY=Math.max(0,s.optInt("scrollY",0));a.editorScroll.getViewTreeObserver().addOnGlobalLayoutListener(new android.view.ViewTreeObserver.OnGlobalLayoutListener(){
                public void onGlobalLayout(){a.editorScroll.getViewTreeObserver().removeOnGlobalLayoutListener(this);a.editorScroll.scrollTo(0,scrollY);}
            });
            if(readWave(a)){a.refresh();a.report("Editing session restored.");}else a.rebuildRestoredWaveform(file);
            return true;
        }catch(FileNotFoundException ignored){return false;}catch(Exception e){a.report("Cannot restore session: "+e.getMessage());return false;}
    }
    static double finite(double n,double fallback){return Double.isFinite(n)?n:fallback;}
    static File waveform(MainActivity a,File source){return new File(directory(a),source.getName()+".wave");}
    static void writeWave(MainActivity a){
        AtomicFile atomic=new AtomicFile(waveform(a,a.input));FileOutputStream raw=null;
        try{raw=atomic.startWrite();DataOutputStream out=new DataOutputStream(new BufferedOutputStream(raw,65536));out.writeInt(a.waveform.length);out.writeDouble(a.peakDb);for(float p:a.waveform)out.writeFloat(p);out.flush();atomic.finishWrite(raw);}
        catch(Exception e){if(raw!=null)atomic.failWrite(raw);}
    }
    static boolean readWave(MainActivity a){
        try(DataInputStream in=new DataInputStream(new BufferedInputStream(new AtomicFile(waveform(a,a.input)).openRead()))){
            int count=in.readInt();if(count<1||count>400000)return false;double peak=in.readDouble();float[] data=new float[count];
            for(int i=0;i<count;i++){data[i]=in.readFloat();if(!Float.isFinite(data[i])||data[i]<0)return false;}
            a.waveform=data;a.peakDb=peak;return true;
        }catch(IOException e){return false;}
    }
}
