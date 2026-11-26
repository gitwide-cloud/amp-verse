package cloud.gitwide.ampverse.lab;
import android.app.AlertDialog;
import android.content.Intent;
import android.net.Uri;
import android.text.*;
import android.text.style.ForegroundColorSpan;
import android.widget.*;
import androidx.core.content.FileProvider;
import cloud.gitwide.ampverse.ModuleRpc;
import com.arthenica.ffmpegkit.*;
import org.json.*;
import java.io.*;

/** Render an edited clip, then copy it into the core through an approved content URI. */
final class SamplerExport {
    static void choose(MainActivity a){
        if(a.input==null||a.busy)return;
        if(a.end-a.start<=0||a.end-a.start>5.0001){a.report("Trim the sample to five seconds or shorter, then send it to a pad.");return;}
        new Thread(()->{try{JSONObject slots=ModuleRpc.call(a,"sampler_slots",null);a.handler.post(()->{if(a.isDestroyed())return;SpannableString amber=new SpannableString("Sampler A"),cyan=new SpannableString("Sampler B");amber.setSpan(new ForegroundColorSpan(0xffffbf24),0,amber.length(),0);cyan.setSpan(new ForegroundColorSpan(0xff22d4ed),0,cyan.length(),0);new AlertDialog.Builder(a).setTitle("Send edited clip to sampler").setItems(new CharSequence[]{amber,cyan},(d,side)->pads(a,slots.optJSONArray("sides").optJSONArray(side),side)).setNegativeButton("Cancel",null).show();});}catch(Exception error){a.report("Sampler: "+error.getMessage());}},"AMP sampler pads").start();
    }
    private static void pads(MainActivity a,JSONArray pads,int side){String[] names=new String[9];for(int i=0;i<9;i++){JSONObject pad=pads.optJSONObject(i);names[i]=(i+1)+" · "+pad.optString("name")+(pad.optBoolean("loaded")?" · replace":" · empty");}new AlertDialog.Builder(a).setTitle("Sampler "+(side==0?"A":"B")).setItems(names,(d,index)->settings(a,pads.optJSONObject(index),side,index)).setNegativeButton("Cancel",null).show();}
    private static void settings(MainActivity a,JSONObject pad,int side,int index){LinearLayout form=a.column();EditText name=new EditText(a);name.setSingleLine(true);name.setText(a.name.replaceFirst("(?i)\\.[^.]+$","")+" clip");name.setTextColor(Retro.appearance.text);name.setContentDescription("Sampler clip name");a.add(form,name);Switch loop=new Switch(a);loop.setText("Loop sample");loop.setTextColor(Retro.appearance.text);loop.setMinimumHeight(a.dp(48));a.add(form,loop);a.add(form,a.label(pad.optBoolean("loaded")?"This replaces the selected pad; its source file is retained.":"The song and your edit stay unchanged.",13,Retro.appearance.muted));AlertDialog dialog=new AlertDialog.Builder(a).setTitle((pad.optBoolean("loaded")?"Replace":"Save to")+" sampler "+(side==0?"A":"B")+" · "+(index+1)).setView(form).setPositiveButton("Send clip",null).setNegativeButton("Cancel",null).create();dialog.setOnShowListener(d->dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(v->{String title=name.getText().toString().trim();if(title.isEmpty()||title.length()>160){name.setError("Use 1–160 characters");return;}dialog.dismiss();render(a,side,index,pad.optLong("revision"),title,loop.isChecked());}));dialog.show();}
    private static void render(MainActivity a,int side,int pad,long revision,String title,boolean loop){
        if(a.busy)return;File root=new File(a.getFilesDir(),"lab-sampler-exports");if(!root.isDirectory()&&!root.mkdirs()){a.report("Cannot create sampler export folder");return;}final File output;try{output=File.createTempFile("sample-",".mp3",root);}catch(Exception error){a.report(error.getMessage());return;}
        final String signature=a.editSignature();a.busy=true;a.report("Rendering edited sample…");
        FFmpegKit.executeWithArgumentsAsync(a.editArguments(output,true),session->{Uri uri=null;try{
            if(!ReturnCode.isSuccess(session.getReturnCode()))throw new IOException("Sample rendering failed; source and edit retained");
            uri=FileProvider.getUriForFile(a,a.getPackageName()+".labexports",output);a.grantUriPermission("cloud.gitwide.ampverse",uri,Intent.FLAG_GRANT_READ_URI_PERMISSION);
            JSONObject result=ModuleRpc.call(a,"sampler_import",new JSONObject().put("uri",uri.toString()).put("side",side).put("pad",pad).put("revision",revision).put("name",title).put("loop",loop));a.handler.post(()->{if(!a.isDestroyed()){a.lastExportedEdit=signature;SessionStore.save(a);}});a.report(result.optString("message","Clip saved to sampler"));
        }catch(Exception error){a.report("Sampler export: "+error.getMessage());}
        finally{try{if(uri!=null)a.revokeUriPermission(uri,Intent.FLAG_GRANT_READ_URI_PERMISSION);}catch(SecurityException ignored){}output.delete();a.busy=false;}});
    }
}
