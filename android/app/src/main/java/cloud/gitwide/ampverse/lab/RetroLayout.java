// Derived from MP3 Lab (MIT). Copyright (c) 2026 MP3 Lab contributors.
package cloud.gitwide.ampverse.lab;

import cloud.gitwide.ampverse.R;

import android.graphics.*;
import android.view.*;
import android.widget.*;
import java.util.Locale;

final class RetroLayout {
    static void create(MainActivity a){
        a.getWindow().setStatusBarColor(Retro.appearance.background);a.getWindow().setNavigationBarColor(Retro.appearance.background);
        ScrollView scroll=new ScrollView(a);a.editorScroll=scroll;scroll.setFillViewport(true);scroll.setBackgroundColor(Retro.appearance.background);
        a.root=a.column();a.root.setPadding(a.dp(8),a.dp(10),a.dp(8),a.dp(16));scroll.addView(a.root);
        a.root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(a.dp(8)+i.getSystemWindowInsetLeft(),a.dp(10)+i.getSystemWindowInsetTop(),a.dp(8)+i.getSystemWindowInsetRight(),a.dp(16)+i.getSystemWindowInsetBottom());return i;});
        a.root.addView(a.moduleMenu(),new LinearLayout.LayoutParams(-1,a.dp(46)));
        LinearLayout menu=new LinearLayout(a);menu.setBackground(Retro.plate(false));
        String[] titles={"File","Edit","Tools","Help"};
        for(int n=0;n<titles.length;n++){
            final int which=n;TextView item=a.label(titles[n],17,Color.WHITE);item.setGravity(Gravity.CENTER);
            menu.addView(item,new LinearLayout.LayoutParams(0,a.dp(40),1));
            item.setOnClickListener(v->{if(which==0)a.pick();else if(which==1){a.gainDb=0;a.fadeIn=0;a.fadeOut=0;a.normalize.setChecked(false);a.refresh();}
                else if(which==2)a.licenses("licenses");else about(a);});
        }a.add(a.root,menu);
        LinearLayout display=a.column();display.setPadding(a.dp(10),a.dp(5),a.dp(10),a.dp(5));display.setBackgroundColor(Retro.INK);
        a.clock=a.label("0:00.000 / 0:00.000",23,Retro.LED);a.clock.setTypeface(Typeface.MONOSPACE);
        a.filename=a.label("NO TRACK LOADED",18,Retro.LED);a.add(display,a.clock);a.add(display,a.filename);
        a.add(display,a.label("MP3   256 kbps export   •   LOCAL AUDIO",12,Retro.LED));
        a.meter=new Retro.Meter(a);display.addView(a.meter,new LinearLayout.LayoutParams(-1,a.dp(34)));a.addSection("PLAYER",display);
        LinearLayout transport=new LinearLayout(a);transport.setPadding(a.dp(3),a.dp(6),a.dp(3),a.dp(6));transport.setBackground(Retro.plate(false));
        String[] controls={"Open","A/B","▶","Ⅱ","■"};
        for(int n=0;n<controls.length;n++){
            final int which=n;Button b=new Button(a);b.setText(controls[n]);Retro.button(b);
            b.setContentDescription(new String[]{"Open audio","Switch original or edited preview","Play preview","Pause or resume","Stop playback"}[n]);
            LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(0,a.dp(45),1);p.setMargins(a.dp(3),0,a.dp(3),0);transport.addView(b,p);
            b.setOnClickListener(v->{if(which==0)a.pick();else if(which==1){a.editedPreview=!a.editedPreview;a.scheduleSessionSave();a.report(a.editedPreview?"Edited preview selected":"Original preview selected");}
                else if(which==2)a.preview(a.editedPreview);else if(which==3){if(a.player!=null){if(a.player.isPlaying())a.player.pause();else a.player.start();}}
                else{a.stop();a.report("Stopped");}});
        }a.add(a.root,transport);
        a.waveformView=new Retro.Wave(a);a.waveformView.selection=(start,end)->{a.stop();a.start=start;a.end=end;a.refresh();a.waveformView.position=a.waveformView.targetStart?start:end;};
        a.waveformView.seek=a::seekPlayback;
        a.waveformView.viewChanged=()->{a.refreshZoom();a.scheduleSessionSave();};
        LinearLayout waveBox=a.column();waveBox.setPadding(a.dp(5),a.dp(6),a.dp(5),a.dp(5));waveBox.setBackground(Retro.plate(false));
        waveBox.addView(a.waveformView,new LinearLayout.LayoutParams(-1,a.dp(176)));
        LinearLayout zoomControls=new LinearLayout(a);String[] zoomTitles={"IN","OUT","−","+","Fit","Seek"};
        for(int n=0;n<zoomTitles.length;n++){
            final int which=n;Button button=new Button(a);button.setText(zoomTitles[n]);Retro.button(button);
            button.setContentDescription(new String[]{"Select and focus IN point","Select and focus OUT point","Zoom out","Zoom in","Show whole track","Tap waveform to seek playback"}[n]);
            LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(0,a.dp(42),1);p.setMargins(a.dp(2),a.dp(3),a.dp(2),a.dp(3));zoomControls.addView(button,p);
            if(n==0)a.inTarget=button;if(n==1)a.outTarget=button;if(n==5)a.seekTarget=button;
            button.setOnClickListener(v->{if(which==0)a.waveformView.focusMarker(true);else if(which==1)a.waveformView.focusMarker(false);else if(which==2)a.waveformView.zoomBy(.5);else if(which==3)a.waveformView.zoomBy(2);else if(which==4)a.waveformView.fit();else{a.waveformView.placing=false;a.refreshZoom();}});
        }a.add(waveBox,zoomControls);
        LinearLayout precision=new LinearLayout(a);
        for(int direction:new int[]{-1,1}){Button nudge=new Button(a);Retro.button(nudge);nudge.setText(direction<0?"−1 ms":"+1 ms");
            LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(0,a.dp(38),1);p.setMargins(a.dp(2),a.dp(2),a.dp(2),a.dp(2));precision.addView(nudge,p);
            nudge.setOnClickListener(v->a.waveformView.nudge(direction*.001));}
        a.add(waveBox,precision);a.zoomLabel=a.label("1×",12,Retro.BLUE);a.add(waveBox,a.zoomLabel);
        a.add(waveBox,a.label("Pinch or +/− to zoom; drag to pan. Seek: tap to hear. IN/OUT: tap to trim.",12,0xffb0bac5));a.add(a.root,waveBox);
        LinearLayout clip=a.column();a.clipLabel=a.label("Start 0:00   End 0:00",16,Color.WHITE);a.clipLabel.setTypeface(Typeface.MONOSPACE);a.clipLabel.setBackgroundColor(Retro.INK);a.add(clip,a.clipLabel);
        a.clipLabel.setOnClickListener(v->new android.app.AlertDialog.Builder(a).setTitle("Trim time").setItems(new String[]{"Start","End"},(d,index)->{
            if(index==0)number(a,"Clip start",a.start,0,a.duration,x->{a.start=Math.min(x,a.end);a.refresh();});else number(a,"Clip end",a.end,a.start,a.duration,x->{a.end=x;a.refresh();});}).show());
        TextView in=a.label("Start  •  tap to enter seconds",13,0xffc1c6cd);in.setOnClickListener(v->number(a,"Clip start",a.start,0,a.duration,x->{a.start=Math.min(x,a.end);a.refresh();}));a.add(clip,in);
        a.inBar=a.seek(clip,0,p->{a.start=Math.min(a.end,a.duration*p/1000);a.refresh();});
        TextView out=a.label("End  •  tap to enter seconds",13,0xffc1c6cd);out.setOnClickListener(v->number(a,"Clip end",a.end,a.start,a.duration,x->{a.end=x;a.refresh();}));a.add(clip,out);
        a.outBar=a.seek(clip,1000,p->{a.end=Math.max(a.start,a.duration*p/1000);a.refresh();});in.setVisibility(View.GONE);out.setVisibility(View.GONE);a.inBar.setVisibility(View.GONE);a.outBar.setVisibility(View.GONE);a.add(clip,a.label("Drag yellow handles or tap times to edit",12,0xffb0bac5));a.addSection("Clip",clip);
        LinearLayout fades=new LinearLayout(a),fadeInGroup=a.column(),fadeOutGroup=a.column();fades.addView(fadeInGroup,new LinearLayout.LayoutParams(0,-2,1));fades.addView(fadeOutGroup,new LinearLayout.LayoutParams(0,-2,1));
        a.fadeInLabel=a.label("Fade In: 0.00 s",15,Color.WHITE);a.add(fadeInGroup,a.fadeInLabel);
        a.fadeInLabel.setOnClickListener(v->number(a,"Fade in",a.fadeIn,0,a.end-a.start,x->{a.fadeIn=x;a.refresh();}));
        a.fadeInBar=a.seek(fadeInGroup,0,p->{a.fadeIn=30*p/1000.0;a.refresh();});
        a.fadeOutLabel=a.label("Fade Out: 0.00 s",15,Color.WHITE);a.add(fadeOutGroup,a.fadeOutLabel);
        a.fadeOutLabel.setOnClickListener(v->number(a,"Fade out",a.fadeOut,0,a.end-a.start-a.fadeIn,x->{a.fadeOut=x;a.refresh();}));
        a.fadeOutBar=a.seek(fadeOutGroup,0,p->{a.fadeOut=30*p/1000.0;a.refresh();});a.addSection("Fades",fades);
        LinearLayout gains=a.column();a.normalize=new CheckBox(a);a.normalize.setText("Normalize  (Target: −16.0 LUFS)");Retro.text(a.normalize);
        a.normalize.setButtonTintList(android.content.res.ColorStateList.valueOf(0xff4d83ff));a.add(gains,a.normalize);
        a.normalize.setOnCheckedChangeListener((button,checked)->a.scheduleSessionSave());
        a.gainLabel=a.label("Gain: +0.0 dB",16,Color.WHITE);a.add(gains,a.gainLabel);
        a.gainLabel.setOnClickListener(v->number(a,"Gain (dB)",a.gainDb,-24,12,x->{a.gainDb=x;a.gainBar.setProgress((int)((x+24)/36*1000));a.refresh();}));
        a.gainBar=a.seek(gains,667,p->{a.gainDb=-24+36.0*p/1000;a.refresh();});
        a.button(gains,"▥  Analyze peak",v->a.report(Double.isNaN(a.peakDb)?"Open audio to analyze peak":String.format(Locale.US,"Source peak: %.1f dBFS",a.peakDb)));a.addSection("Gain",gains);
        LinearLayout footer=a.column();footer.setPadding(a.dp(5),a.dp(6),a.dp(5),a.dp(6));footer.setBackground(Retro.plate(false));
        a.button(footer,"♫  Export MP3…   |   256 kbps",v->a.save());a.status=a.label("Ready. Open an audio file.",14,0xffeeeeed);a.add(footer,a.status);a.add(a.root,footer);
        a.setContentView(scroll);a.refresh();
    }
    static void about(MainActivity a){
        ImageView logo=new ImageView(a);logo.setImageResource(R.drawable.app_icon);logo.setAdjustViewBounds(true);logo.setBackgroundColor(Color.WHITE);
        new android.app.AlertDialog.Builder(a).setTitle("AMP Lab · MP3 Lab foundation").setView(logo).setMessage("Local audio editor • 256 kbps MP3\nMIT licensed • Independent of Winamp")
            .setPositiveButton("Close",null).setNeutralButton("Licenses",(d,w)->a.licenses("licenses")).show();
    }
    static void number(MainActivity a,String title,double value,double min,double max,java.util.function.DoubleConsumer set){
        EditText edit=new EditText(a);edit.setInputType(android.text.InputType.TYPE_CLASS_NUMBER|android.text.InputType.TYPE_NUMBER_FLAG_DECIMAL|android.text.InputType.TYPE_NUMBER_FLAG_SIGNED);
        edit.setText(String.format(Locale.US,"%.3f",value));edit.selectAll();
        new android.app.AlertDialog.Builder(a).setTitle(title).setMessage("Seconds unless indicated otherwise").setView(edit).setNegativeButton("Cancel",null)
            .setPositiveButton("Apply",(d,w)->{try{double x=Double.parseDouble(edit.getText().toString());if(Double.isFinite(x))set.accept(Math.max(min,Math.min(max,x)));}catch(NumberFormatException e){a.report("Enter a valid number");}}).show();
    }
}
