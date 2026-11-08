// Derived from MP3 Lab (MIT). Copyright (c) 2026 MP3 Lab contributors.
package cloud.gitwide.ampverse.lab;

import cloud.gitwide.ampverse.R;

import android.app.Activity;
import android.content.Intent;
import android.graphics.Color;
import android.media.MediaMetadataRetriever;
import android.media.MediaPlayer;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.OpenableColumns;
import android.database.Cursor;
import android.view.View;
import android.widget.*;
import java.io.*;
import java.util.Locale;
import com.arthenica.ffmpegkit.FFmpegKit;
import com.arthenica.ffmpegkit.ReturnCode;

public class MainActivity extends Activity {
    private String themeId;
    static final int PICK=10, SAVE=11;
    Button moduleMenu(){
        Button brand=new Button(this);brand.setText("ϟ AMP VERSE ▾ · AMP Lab");Retro.button(brand);brand.setContentDescription("AMP Verse module dropdown");
        brand.setOnClickListener(v->{PopupMenu menu=new PopupMenu(this,brand);for(String name:new String[]{"AMP Player","AMP Library","AMP DJ","AMP Remote","AMP Lab","AMP Cast","Settings"})menu.getMenu().add(name);menu.setOnMenuItemClickListener(item->{String name=item.getTitle().toString();if(name.equals("AMP Lab"))return true;if(name.equals("Settings"))startActivity(new Intent(this,cloud.gitwide.ampverse.SettingsActivity.class));else{startActivity(new Intent(this,cloud.gitwide.ampverse.MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP|Intent.FLAG_ACTIVITY_SINGLE_TOP).putExtra("module",name));finish();}return true;});menu.show();});return brand;
    }
    static final int BG=0xff1d2027, PANEL=0xff303640, LED=0xff90f569;
    final Handler handler=new Handler(Looper.getMainLooper());
    LinearLayout root;
    TextView status,filename,clock,gainLabel,fadeInLabel,fadeOutLabel;
    SeekBar inBar,outBar,gainBar,fadeInBar,fadeOutBar;
    CheckBox normalize;
    File input;
    String name="song";
    double duration=0,start=0,end=0,fadeIn=0,fadeOut=0,gainDb=0;
    MediaPlayer player;
    volatile boolean busy;
    volatile int previewGeneration;
    Retro.Wave waveformView;
    float[] waveform=new float[0];
    double peakDb=Double.NaN;
    TextView clipLabel;
    boolean editedPreview=true;
    Retro.Meter meter;
    ScrollView editorScroll;
    TextView zoomLabel;
    Button inTarget,outTarget,seekTarget;
    double playbackStart,playbackEnd,pendingSeek;
    final Runnable checkpoint=()->SessionStore.save(this);
    final Runnable playbackTick=new Runnable(){public void run(){
        if(player!=null&&waveformView!=null){try{waveformView.position=playbackStart+player.getCurrentPosition()/1000.0;waveformView.invalidate();clock.setText(time(waveformView.position)+" / "+time(duration));if(meter!=null)meter.invalidate();}catch(IllegalStateException ignored){}}
        handler.postDelayed(this,80);
    }};
    int dp(int x){return (int)(x*getResources().getDisplayMetrics().density);}
    TextView label(String text,int size,int color) {
        TextView v=new TextView(this);v.setText(text);v.setTextSize(size);Retro.text(v);v.setTextColor(color);v.setPadding(dp(4),dp(4),dp(4),dp(4));return v;
    }
    LinearLayout column(){LinearLayout l=new LinearLayout(this);l.setOrientation(LinearLayout.VERTICAL);return l;}
    void add(LinearLayout l,View v){l.addView(v,new LinearLayout.LayoutParams(-1,-2));}
    void addSection(String heading,View body){
        LinearLayout block=column();block.setPadding(dp(10),dp(5),dp(10),dp(8));block.setBackground(Retro.plate(false));
        add(block,label(heading,16,0xff7ab6e8));add(block,body);
        LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(-1,-2);p.bottomMargin=dp(6);root.addView(block,p);
    }
    SeekBar seek(LinearLayout parent,int initial,java.util.function.IntConsumer callback){
        SeekBar b=new SeekBar(this);b.setMax(1000);b.setProgress(initial);
        b.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){
            public void onProgressChanged(SeekBar bar,int progress,boolean fromUser){if(fromUser)callback.accept(progress);}
            public void onStartTrackingTouch(SeekBar bar){}
            public void onStopTrackingTouch(SeekBar bar){}
        });
        Retro.seek(b,Math.max(1,dp(1)));add(parent,b);return b;
    }
    Button button(LinearLayout l,String title,View.OnClickListener onClick){
        Button b=new Button(this);b.setText(title);Retro.button(b);b.setOnClickListener(onClick);
        LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(-1,dp(45));p.setMargins(dp(3),dp(3),dp(3),dp(3));l.addView(b,p);return b;
    }
    @Override public void onCreate(Bundle state){super.onCreate(state);Retro.configure(this);themeId=getSharedPreferences("settings",MODE_PRIVATE).getString("theme","classic");RetroLayout.create(this);
        if(!SessionStore.restore(this,state==null?null:state.getString("editingSession"))&&state!=null)SessionStore.restore(this,null);
    }
    void scheduleSessionSave(){handler.removeCallbacks(checkpoint);if(input!=null)handler.postDelayed(checkpoint,180);}
    void refreshZoom(){
        if(zoomLabel!=null)zoomLabel.setText(String.format(Locale.US,"%.1f×   %s – %s",waveformView.zoom,time(waveformView.viewStart),time(waveformView.visibleEnd())));
        if(inTarget!=null){inTarget.setText(waveformView.placing&&waveformView.targetStart?"IN *":"IN");outTarget.setText(waveformView.placing&&!waveformView.targetStart?"OUT *":"OUT");seekTarget.setText(waveformView.placing?"Seek":"Seek *");}
    }
    String time(double s){
        int seconds=(int)Math.max(0,s);
        return String.format(Locale.US,"%d:%06.3f",seconds/60,s-(seconds/60)*60);
    }
    void refresh(){
        start=Math.max(0,Math.min(duration,start));end=Math.max(start,Math.min(duration,end));gainDb=Math.max(-24,Math.min(12,gainDb));fadeIn=Math.max(0,fadeIn);fadeOut=Math.max(0,fadeOut);
        double len=Math.max(0,end-start);
        fadeIn=Math.min(fadeIn,len);
        fadeOut=Math.min(fadeOut,Math.max(0,len-fadeIn));
        if(fadeInBar!=null)fadeInBar.setProgress((int)(fadeIn/30*1000));
        if(fadeOutBar!=null)fadeOutBar.setProgress((int)(fadeOut/30*1000));
        if(duration>0){inBar.setProgress((int)(start/duration*1000));outBar.setProgress((int)(end/duration*1000));}
        if(clock!=null)clock.setText(time(start)+" / "+time(duration));
        if(fadeInLabel!=null)fadeInLabel.setText(String.format(Locale.US,"Fade In  %.2f s",fadeIn));
        if(fadeOutLabel!=null)fadeOutLabel.setText(String.format(Locale.US,"Fade Out  %.2f s",fadeOut));
        if(gainLabel!=null)gainLabel.setText(String.format(Locale.US,"Gain: %+.1f dB",gainDb));
        if(clipLabel!=null)clipLabel.setText("Start    "+time(start)+"\nEnd      "+time(end)+"\nLength   "+time(len));
        if(waveformView!=null){waveformView.peaks=waveform;waveformView.duration=duration;waveformView.start=start;waveformView.end=end;waveformView.position=Math.max(0,Math.min(duration,waveformView.position));waveformView.invalidate();}
        if(meter!=null)meter.invalidate();
        if(input!=null)getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        refreshZoom();scheduleSessionSave();
    }
    void report(String msg){handler.post(()->status.setText(msg));}
    void licenses(String path){
        try {
            String[] children=getAssets().list(path);
            if(children!=null&&children.length>0){
                new android.app.AlertDialog.Builder(this).setTitle("Open source licenses")
                    .setItems(children,(d,index)->licenses(path+"/"+children[index])).setNegativeButton("Close",null).show();
            } else {
                String text;
                try(InputStream in=getAssets().open(path);ByteArrayOutputStream out=new ByteArrayOutputStream()){
                    copy(in,out);text=out.toString("UTF-8");
                }
                ScrollView scroll=new ScrollView(this);TextView view=label(text,13,Color.WHITE);view.setTextIsSelectable(true);scroll.addView(view);
                new android.app.AlertDialog.Builder(this).setTitle(path.substring(path.lastIndexOf('/')+1)).setView(scroll).setPositiveButton("Close",null).show();
            }
        }catch(IOException ex){report("Cannot open license: "+ex.getMessage());}
    }
    void pick(){
        if(busy)return;
        Intent i=new Intent(Intent.ACTION_OPEN_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);i.setType("audio/*");startActivityForResult(i,PICK);
    }
    @Override protected void onActivityResult(int req,int result,Intent data){
        super.onActivityResult(req,result,data);
        if(result!=RESULT_OK||data==null||data.getData()==null)return;
        if(req==PICK)load(data.getData());
        if(req==SAVE)export(data.getData());
    }
    void load(Uri uri){
        if(busy)return;busy=true;stop();report("Loading audio...");
        final String displayName=displayName(uri);
        final File file;
        try{file=File.createTempFile("track_",".audio",SessionStore.directory(this));}
        catch(Exception e){busy=false;report("Cannot create editing copy: "+e.getMessage());return;}
        new Thread(()->{
            try {
                try(InputStream in=getContentResolver().openInputStream(uri);
                    FileOutputStream out=new FileOutputStream(file)){
                    if(in==null)throw new IOException("Cannot open file");
                    copy(in,out);out.getFD().sync();
                }
                MediaMetadataRetriever r=new MediaMetadataRetriever();String length;
                try{r.setDataSource(file.getAbsolutePath());length=r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION);}finally{r.release();}
                double seconds=length==null?0:Double.parseDouble(length)/1000.0;
                if(seconds<=0||!Double.isFinite(seconds))throw new IOException("Duration not available");
                float[] peaks=readWaveform(file,seconds);
                handler.post(()->{
                    if(isDestroyed()){file.delete();busy=false;return;}
                    File previous=input;name=displayName;
                    input=file;waveform=peaks;duration=seconds;start=0;end=seconds;fadeIn=0;fadeOut=0;gainDb=0;
                    inBar.setProgress(0);outBar.setProgress(1000);
                    fadeInBar.setProgress(0);fadeOutBar.setProgress(0);gainBar.setProgress(667);
                    normalize.setChecked(false);filename.setText(name);waveformView.fit();refresh();SessionStore.writeWave(this);
                    boolean saved=SessionStore.save(this);
                    if(saved&&previous!=null&&!previous.equals(file)){
                        try{if(SessionStore.owns(this,previous)){previous.delete();SessionStore.waveform(this,previous).delete();}}catch(IOException ignored){}
                    }
                    report(saved?"Loaded "+name:"Loaded; session autosave unavailable");busy=false;
                });
            }catch(Exception ex){file.delete();report("Load error: "+ex.getMessage());busy=false;}
        }).start();
    }
    String displayName(Uri uri){
        try(Cursor cursor=getContentResolver().query(uri,new String[]{OpenableColumns.DISPLAY_NAME},null,null,null)){
            if(cursor!=null&&cursor.moveToFirst()){String title=cursor.getString(0);if(title!=null&&!title.trim().isEmpty())return title;}
        }catch(Exception ignored){}
        return "song";
    }
    String quote(String s){return "'"+s.replace("'","'\\''")+"'";}
    String filters(){
        StringBuilder f=new StringBuilder("asetpts=PTS-STARTPTS");
        if(normalize.isChecked())f.append(",loudnorm=I=-16:TP=-3:LRA=11");
        if(Math.abs(gainDb)>0.001)f.append(String.format(Locale.US,",volume=%+.2fdB",gainDb));
        if(fadeIn>0)f.append(String.format(Locale.US,",afade=t=in:st=0:d=%.3f",fadeIn));
        if(fadeOut>0)f.append(String.format(Locale.US,",afade=t=out:st=%.3f:d=%.3f",end-start-fadeOut,fadeOut));
        return f.append(",alimiter=limit=0.95:attack=5:release=50:level=0").toString();
    }
    String command(File output,boolean edited){
        StringBuilder b=new StringBuilder(String.format(Locale.US,"-y -nostdin -hide_banner -loglevel error -ss %.5f ",start));
        b.append("-i ").append(quote(input.getAbsolutePath()));
        b.append(String.format(Locale.US," -t %.5f -map 0:a:0 -vn -map_metadata -1 ",end-start));
        if(edited)b.append("-af ").append(quote(filters())).append(' ');
        return b.append("-codec:a libmp3lame -b:a 256k -ar 44100 -ac 2 -f mp3 ").append(quote(output.getAbsolutePath())).toString();
    }
    void preview(boolean edited){
        if(input==null||busy)return;
        if(end-start<.05){report("Choose a longer clip");return;}
        double requested=waveformView.position;
        stop();playbackStart=start;playbackEnd=end;pendingSeek=Math.max(start,Math.min(end-.001,requested));busy=true;report("Preparing preview...");
        int generation=previewGeneration;
        File dest=new File(getCacheDir(),"mp3_lab_preview.mp3");
        FFmpegKit.executeAsync(command(dest,edited),session->{
            if(!ReturnCode.isSuccess(session.getReturnCode())){
                report("Preview encoding failed");busy=false;return;
            }
            handler.post(()->{
                if(generation!=previewGeneration||isDestroyed()){busy=false;return;}
                try{
                    player=new MediaPlayer();player.setDataSource(dest.getAbsolutePath());
                    player.prepare();player.setOnCompletionListener(mp->{stop();report("Preview finished");});
                    player.seekTo((long)((pendingSeek-playbackStart)*1000),MediaPlayer.SEEK_CLOSEST);
                    player.start();report(edited?"Playing edited preview":"Playing original clip");
                }catch(Exception e){report(e.getMessage());}finally{busy=false;}
            });
        });
    }
    void seekPlayback(double seconds){
        double lo=player!=null||busy?playbackStart:start,hi=player!=null||busy?playbackEnd:end;
        double target=Math.max(lo,Math.min(Math.max(lo,hi-.001),seconds));pendingSeek=target;
        if(player!=null)try{player.seekTo((long)((target-playbackStart)*1000),MediaPlayer.SEEK_CLOSEST);}catch(IllegalStateException e){report("Seek failed: "+e.getMessage());return;}
        waveformView.position=target;waveformView.invalidate();clock.setText(time(target)+" / "+time(duration));
        if(target!=seconds)report("Preview seeks within the selected clip. Move IN/OUT to include more audio.");
    }
    void stop(){
        previewGeneration++;
        if(player!=null){try{player.stop();}catch(Exception ignored){}player.release();player=null;}
    }
    void save(){
        if(input==null||busy)return;
        if(end-start<.05){report("Choose a longer clip");return;}
        Intent i=new Intent(Intent.ACTION_CREATE_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);i.setType("audio/mpeg");
        i.putExtra(Intent.EXTRA_TITLE,name.replaceFirst("(?i)\\.[^.]+$","")+"_clip.mp3");
        startActivityForResult(i,SAVE);
    }
    void export(Uri uri){
        if(input==null||busy)return;
        busy=true;report("Exporting MP3 at 256 kbps...");
        File temp=new File(getCacheDir(),"mp3_lab_export.mp3");
        FFmpegKit.executeAsync(command(temp,true),session->{
            if(!ReturnCode.isSuccess(session.getReturnCode())){report("Encoding failed");busy=false;return;}
            new Thread(()->{
                try{
                    try(InputStream in=new FileInputStream(temp);OutputStream out=getContentResolver().openOutputStream(uri,"wt")){
                        if(out==null)throw new IOException("Output not writable");copy(in,out);out.flush();
                    }
                    MediaMetadataRetriever check=new MediaMetadataRetriever();
                    try{check.setDataSource(this,uri);String length=check.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION);
                        if(length==null||Long.parseLong(length)<=0)throw new IOException("Saved file has no playable audio");
                    }finally{check.release();}
                    report("Saved and verified 256 kbps MP3");
                }catch(Exception e){report("Save failed: "+e.getMessage());}
                finally{busy=false;temp.delete();}
            }).start();
        });
    }
    static void copy(InputStream in,OutputStream out)throws IOException{
        byte[] data=new byte[65536];int n;
        while((n=in.read(data))!=-1)out.write(data,0,n);
    }
    float[] readWaveform(File file,double seconds)throws IOException{
        File pcm=File.createTempFile("waveform_",".pcm",getCacheDir());
        try{
            com.arthenica.ffmpegkit.FFmpegSession session=FFmpegKit.execute("-y -hide_banner -i "+quote(file.toString())+" -filter_complex "+quote("[0:a]asplit=2[wave][stats];[stats]astats=metadata=0:reset=0[meter]")+" -map [wave] -ac 1 -ar 2000 -f f32le "+quote(pcm.toString())+" -map [meter] -f null -");
            if(!ReturnCode.isSuccess(session.getReturnCode()))return new float[0];
            float[] peaks=new float[(int)Math.min(400000,Math.max(1200,Math.ceil(seconds*2000)))];float peak=0;long index=0;
            try(DataInputStream in=new DataInputStream(new BufferedInputStream(new FileInputStream(pcm)))){
                while(in.available()>=4){float v=Math.abs(Float.intBitsToFloat(Integer.reverseBytes(in.readInt())));int bin=(int)(index++/Math.max(1,seconds*2000)*peaks.length);if(bin<peaks.length)peaks[bin]=Math.max(peaks[bin],v);peak=Math.max(peak,v);}
            }peakDb=peak>0?20*Math.log10(peak):Double.NEGATIVE_INFINITY;
            java.util.regex.Matcher meter=java.util.regex.Pattern.compile("Peak level dB:\\s+(-?[0-9.]+|-inf)").matcher(session.getAllLogsAsString());
            while(meter.find())peakDb=meter.group(1).equals("-inf")?Double.NEGATIVE_INFINITY:Double.parseDouble(meter.group(1));
            return peaks;
        }finally{pcm.delete();}
    }
    void rebuildRestoredWaveform(File source){
        busy=true;report("Session restored. Reading waveform…");
        new Thread(()->{try{float[] restored=readWaveform(source,duration);handler.post(()->{
            if(!isDestroyed()&&source.equals(input)){waveform=restored;SessionStore.writeWave(this);refresh();report("Editing session restored.");}busy=false;
        });}catch(Exception e){busy=false;report("Session restored; waveform unavailable: "+e.getMessage());}}).start();
    }
    @Override protected void onSaveInstanceState(Bundle state){
        handler.removeCallbacks(checkpoint);SessionStore.save(this);
        try{org.json.JSONObject saved=SessionStore.snapshot(this);if(saved!=null)state.putString("editingSession",saved.toString());}catch(Exception ignored){}
        super.onSaveInstanceState(state);
    }
    @Override protected void onDestroy(){handler.removeCallbacks(checkpoint);stop();super.onDestroy();}
    @Override protected void onResume(){super.onResume();String next=getSharedPreferences("settings",MODE_PRIVATE).getString("theme","classic");if(!next.equals(themeId)){String snapshot=null;try{org.json.JSONObject saved=SessionStore.snapshot(this);if(saved!=null)snapshot=saved.toString();}catch(Exception ignored){}themeId=next;Retro.configure(this);RetroLayout.create(this);if(snapshot!=null)SessionStore.restore(this,snapshot);}handler.post(playbackTick);}
    @Override protected void onPause(){handler.removeCallbacks(playbackTick);handler.removeCallbacks(checkpoint);SessionStore.save(this);stop();super.onPause();}
}
