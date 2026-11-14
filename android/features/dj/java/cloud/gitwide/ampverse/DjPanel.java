package cloud.gitwide.ampverse;

import android.app.*;
import android.content.*;
import android.graphics.*;
import android.view.*;
import android.widget.*;
import androidx.media3.common.*;
import androidx.media3.exoplayer.ExoPlayer;
import java.util.*;

/** Responsive native controls around the service-owned DJ session. */
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
final class DjPanel extends LinearLayout {
    interface Host {void effect(int side,int pad);void beforeMainPlay();void equalizer();}
    private final Activity activity;
    private final Appearance colors;
    final DjEngine engine;
    private final Host host;
    private final TextView[] info=new TextView[4];
    private final SeekBar[] timelines=new SeekBar[4];
    private TextView status;
    private SeekBar fader;
    private Button mix;
    private Switch autoToggle;
    private boolean updatingAuto;
    private int visibleDeck;
    private final android.os.Handler timer=new android.os.Handler(android.os.Looper.getMainLooper());
    private final Runnable tick=new Runnable(){public void run(){refresh();timer.postDelayed(this,250);}};
    private LinearLayout pinnedMixer;
    View compactMixer(){return getResources().getConfiguration().screenWidthDp<700?pinnedMixer:null;}
    DjPanel(Activity activity,Appearance colors,DjEngine engine,Host host){super(activity);this.activity=activity;this.colors=colors;this.engine=engine;this.host=host;setOrientation(VERTICAL);visibleDeck=engine.destination;pinnedMixer=new LinearLayout(activity);pinnedMixer.setPadding(dp(6),dp(4),dp(6),dp(4));pinnedMixer.setGravity(Gravity.CENTER_VERTICAL);pinnedMixer.setBackground(Chrome.panel(colors,false));draw();}
    private int dp(int n){return (int)(n*getResources().getDisplayMetrics().density);}
    private TextView label(String value,int size){TextView v=new TextView(activity);v.setText(value);v.setTextSize(size);v.setTextColor(colors.text);v.setPadding(dp(4),dp(6),dp(4),dp(6));return v;}
    private Button button(String text,int color,Runnable action){return decorate(new Button(activity),text,color,action);}
    private int readable(int color){if(Color.red(colors.background)>200){if(color==DjEngine.AMBER)return 0xff8a5c00;if(color==DjEngine.CYAN)return 0xff007b8a;}return color;}
    private Button decorate(Button b,String text,int color,Runnable action){b.setText(text);b.setAllCaps(false);b.setTextColor(readable(color));b.setTextSize(13);b.setMinWidth(0);b.setMinimumWidth(0);b.setPadding(dp(5),0,dp(5),0);b.setMinHeight(dp(48));b.setBackground(Chrome.button(colors,false));b.setContentDescription(text);b.setOnClickListener(v->{action.run();refresh();});return b;}
    private LinearLayout card(LinearLayout parent){LinearLayout v=new LinearLayout(activity);v.setOrientation(VERTICAL);v.setPadding(dp(8),dp(8),dp(8),dp(8));v.setBackground(Chrome.panel(colors,false));LayoutParams p=new LayoutParams(-1,-2);p.setMargins(0,dp(8),0,0);parent.addView(v,p);return v;}
    private void row(LinearLayout parent,Button...buttons){LinearLayout row=new LinearLayout(activity);for(Button b:buttons){LayoutParams p=new LayoutParams(0,dp(50),1);p.setMargins(dp(2),dp(2),dp(2),dp(2));row.addView(b,p);}parent.addView(row);}
    private SeekBar slider(LinearLayout parent,int max,int value,java.util.function.IntConsumer changed){SeekBar bar=new SeekBar(activity);Chrome.slider(bar,colors.accent);bar.setMax(max);bar.setProgress(value);bar.setMinimumHeight(dp(48));bar.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){}public void onStopTrackingTouch(SeekBar b){}public void onProgressChanged(SeekBar b,int p,boolean user){if(user)changed.accept(p);}});parent.addView(bar,new LayoutParams(-1,dp(48)));return bar;}
    private void draw(){
        removeAllViews();Arrays.fill(info,null);Arrays.fill(timelines,null);
        boolean wide=getResources().getConfiguration().screenWidthDp>=700;
        LinearLayout toolbar=card(this);if(wide){toolbar.setOrientation(HORIZONTAL);toolbar.setPadding(dp(8),dp(4),dp(8),dp(4));}
        Button source=button("Choose playlist",colors.accent,this::chooseAutoSource);if(wide)toolbar.addView(source,new LayoutParams(0,dp(50),1));else row(toolbar,source);
        autoToggle=new Switch(activity);autoToggle.setText("Auto Mix");autoToggle.setTextColor(colors.text);Chrome.toggle(autoToggle,colors);autoToggle.setMinHeight(dp(48));autoToggle.setContentDescription("Phone Auto Mix");autoToggle.setChecked(engine.auto);autoToggle.setOnCheckedChangeListener((v,on)->{if(updatingAuto)return;if(on&&engine.autoQueue.isEmpty()){updatingAuto=true;autoToggle.setChecked(false);updatingAuto=false;chooseAutoSource(true);return;}if(on)host.beforeMainPlay();engine.auto(on);refresh();});if(wide)toolbar.addView(autoToggle,new LayoutParams(0,dp(50),1));else toolbar.addView(autoToggle);
        status=label(engine.message,13);if(wide){status.setMaxLines(2);status.setEllipsize(android.text.TextUtils.TruncateAt.END);toolbar.addView(status,new LayoutParams(0,dp(50),1));}else toolbar.addView(status);
        LinearLayout mains=new LinearLayout(activity);mains.setOrientation(wide?HORIZONTAL:VERTICAL);addView(mains);
        if(!wide){Button a=button("DECK A",DjEngine.AMBER,()->{visibleDeck=0;engine.destination(0);draw();}),b=button("DECK B",DjEngine.CYAN,()->{visibleDeck=1;engine.destination(1);draw();});Button selected=visibleDeck==0?a:b;selected.setSelected(true);selected.setTextColor(Color.BLACK);selected.setBackgroundTintList(android.content.res.ColorStateList.valueOf(visibleDeck==0?DjEngine.AMBER:DjEngine.CYAN));row(mains,a,b);}
        for(int deck=0;deck<2;deck++){if(!wide&&deck!=visibleDeck)continue;LinearLayout column=new LinearLayout(activity);column.setOrientation(VERTICAL);mains.addView(column,new LayoutParams(wide?0:-1,-2,wide?1:0));deck(column,deck);}
        LinearLayout center;
        pinnedMixer.removeAllViews();
        if(wide){center=card(this);center.addView(label("Crossfader · A ↔ B",18));}
        else{center=pinnedMixer;TextView a=label("A",14);a.setTextColor(readable(DjEngine.AMBER));center.addView(a);}
        fader=slider(center,1000,(int)(engine.fader*1000),p->{if(engine.auto)engine.auto(false);engine.cancelMix();engine.setFader(p/1000f);});fader.setContentDescription("DJ crossfader");
        if(!wide){fader.setLayoutParams(new LayoutParams(0,dp(48),1));TextView b=label("B",14);b.setTextColor(readable(DjEngine.CYAN));center.addView(b);}
        mix=button("Mix next",colors.accent,()->{host.beforeMainPlay();engine.mixNext();});mix.setBackground(Chrome.button(colors,true));mix.setTextColor(Color.BLACK);center.addView(mix,new LayoutParams(wide?-1:dp(108),dp(50)));
        Button settings=button(wide?"Mixer settings":"⋮",colors.text,this::mixerSettings);settings.setContentDescription("DJ mixer settings");center.addView(settings,new LayoutParams(wide?-1:dp(48),dp(50)));
        if(wide){removeView(center);mains.addView(center,1,new LayoutParams(0,-2,.7f));}
        int auxiliaryStart=getChildCount();
        LinearLayout aux=new LinearLayout(activity);aux.setOrientation(wide?HORIZONTAL:VERTICAL);addView(aux);
        for(int side=0;side<2;side++){LinearLayout column=new LinearLayout(activity);column.setOrientation(VERTICAL);aux.addView(column,new LayoutParams(wide?0:-1,-2,wide?1:0));side(column,side);}
        UiSections.fold(this,colors,"Sampler & Cue Decks",auxiliaryStart);
        LinearLayout library=card(this);library.addView(label("DJ Library",20));library.addView(button("Browse all music",colors.text,()->{engine.queue(new ArrayList<>());draw();}));
        EditText search=new EditText(activity);search.setSingleLine(true);search.setHint("Search phone music");search.setTextColor(colors.text);search.setHintTextColor(colors.muted);search.setContentDescription("Search DJ music");library.addView(search);
        LinearLayout songs=new LinearLayout(activity);songs.setOrientation(VERTICAL);library.addView(songs);
        Runnable filter=()->renderSongs(songs,search.getText().toString());
        search.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int s,int c,int a){}public void onTextChanged(CharSequence t,int s,int b,int c){filter.run();}public void afterTextChanged(android.text.Editable e){}});filter.run();refresh();
    }
    private void mixerSettings(){LinearLayout form=new LinearLayout(activity);form.setOrientation(VERTICAL);form.setPadding(dp(12),dp(8),dp(12),dp(8));form.setBackground(Chrome.panel(colors,false));form.addView(label("Master volume",16));slider(form,100,(int)(engine.master*100),p->{engine.master=p/100f;engine.setFader(engine.fader);});TextView timing=label("Crossfade · "+engine.fadeSeconds+" seconds",16);form.addView(timing);row(form,button("−1s",colors.text,()->{engine.fadeSeconds(engine.fadeSeconds-1);timing.setText("Crossfade · "+engine.fadeSeconds+" seconds");refresh();}),button("+1s",colors.text,()->{engine.fadeSeconds(engine.fadeSeconds+1);timing.setText("Crossfade · "+engine.fadeSeconds+" seconds");refresh();}));form.addView(button("Equalizer",colors.text,host::equalizer));ScrollView scroll=new ScrollView(activity);scroll.addView(form);new AlertDialog.Builder(activity).setTitle("DJ mixer").setView(scroll).setPositiveButton("Done",null).show();}
    private void deck(LinearLayout parent,int index){
        int color=index==0?DjEngine.AMBER:DjEngine.CYAN;LinearLayout card=card(parent);TextView heading=label("DECK "+(index==0?"A":"B"),20);heading.setTextColor(readable(color));card.addView(heading);transport(card,index,color);
        Jog jog=new Jog(activity,color,delta->{engine.cancelMix();engine.seek(index,engine.decks[index].getCurrentPosition()+delta);});card.addView(jog,new LayoutParams(-1,dp(110)));

        row(card,button("Sync tempo",color,()->sync(index)),button("Hot cues",color,()->cues(index)),button("Loop",color,()->{ExoPlayer deck=engine.decks[index];deck.setRepeatMode(deck.getRepeatMode()==Player.REPEAT_MODE_OFF?Player.REPEAT_MODE_ONE:Player.REPEAT_MODE_OFF);engine.message=deck.getRepeatMode()==Player.REPEAT_MODE_ONE?"Track loop enabled":"Track loop disabled";}));
        int settings=card.getChildCount();card.addView(label("Pitch ±16%",12));slider(card,32,16,p->engine.decks[index].setPlaybackSpeed(1+(p-16)/100f));drop(card,index);UiSections.fold(card,colors,"Deck settings",settings);
    }
    private void transport(LinearLayout card,int index,int color){
        info[index]=label("No track loaded",15);card.addView(info[index]);
        timelines[index]=slider(card,1000,0,p->{long duration=engine.decks[index].getDuration();if(duration>0)engine.seek(index,duration*p/1000);});Chrome.slider(timelines[index],readable(color));timelines[index].setContentDescription("Seek "+(index<2?"deck "+(index==0?"A":"B"):"cue "+(index-1)));boolean compactTransport=getResources().getConfiguration().screenWidthDp>=700&&index<2;if(compactTransport)card.removeView(timelines[index]);
        row(card,button("Load",color,()->choose(index)),button("Play / Pause",color,()->{if(engine.active(index))engine.decks[index].pause();else{if(index<2)host.beforeMainPlay();engine.play(index);}}),button("Stop",color,()->{if(index<2&&engine.auto)engine.auto(false);engine.stop(index);}));
        ((LinearLayout)card.getChildAt(card.getChildCount()-1)).getChildAt(1).setTag("deck-play-"+index);
        int staging=card.getChildCount();if(compactTransport)card.addView(timelines[index]);row(card,button("−1s",color,()->engine.seek(index,engine.decks[index].getCurrentPosition()-1000)),button("+1s",color,()->engine.seek(index,engine.decks[index].getCurrentPosition()+1000)),button("Apply staged",color,()->engine.apply(index)));UiSections.fold(card,colors,"Seek & staged track",staging);
    }
    private void side(LinearLayout parent,int side){
        int color=side==0?DjEngine.AMBER:DjEngine.CYAN;LinearLayout card=card(parent);
        Button sideButton=button((engine.collapsed[side]?"▸ ":"▾ ")+(side==0?"Left":"Right")+" side deck",color,()->{engine.collapse(side);draw();});sideButton.setContentDescription(side==0?"Left side deck":"Right side deck");card.addView(sideButton);if(engine.collapsed[side])return;
        row(card,button("Sampler",color,()->{engine.mode(side,false);draw();}),button("Cue Deck",color,()->{engine.mode(side,true);draw();}));
        if(engine.cueMode[side]){
            card.addView(label("Cue Deck · independent playback",17));transport(card,side+2,color);slider(card,100,(int)(engine.decks[side+2].getVolume()*100),p->engine.cueVolume(side,p/100f));
            row(card,button("Send → A",DjEngine.AMBER,()->send(side,0)),button("Send → B",DjEngine.CYAN,()->send(side,1)),button("Hot cues",color,()->cues(side+2)));
            card.addView(label("Uses this phone’s current audio output. No separate headphone monitoring on Android.",12));drop(card,side+2);
        }else{
            card.addView(label("Nine effect pads · max 5s · hold to configure",16));slider(card,100,(int)(engine.samplerVolume[side]*100),p->engine.sampleVolume(side,p/100f));
            for(int r=0;r<3;r++){LinearLayout row=new LinearLayout(activity);card.addView(row);for(int c=0;c<3;c++){final int pad=r*3+c;Button b=decorate(new SquarePad(activity),engine.names[side][pad],color,()->{if(engine.samples[side][pad].isEmpty())host.effect(side,pad);else engine.trigger(side,pad);});b.setContentDescription((side==0?"Left":"Right")+" pad "+(pad+1));b.setOnLongClickListener(v->{padMenu(side,pad);return true;});LayoutParams p=new LayoutParams(0,dp(90),1);p.setMargins(dp(2),dp(2),dp(2),dp(2));row.addView(b,p);}}
            card.addView(button("Stop all pads",color,()->{for(int pad=0;pad<9;pad++)engine.stopPad(side,pad);}));
        }
    }
    private void chooseAutoSource(){chooseAutoSource(false);}
    private void chooseAutoSource(boolean startAfterLoad){try{
        org.json.JSONArray lists=PhonePlaylists.read(activity);java.util.List<java.io.File> offline=RadioOffline.albums(activity);java.util.List<String> names=new java.util.ArrayList<>();
        for(int i=0;i<lists.length();i++)names.add(lists.getJSONObject(i).optString("name"));
        for(java.io.File folder:offline)names.add("Offline · "+RadioOffline.album(folder).optString("name","Playlist"));
        if(names.isEmpty()){engine.message="Add a playlist in Library or download one from Cast & Remote.";refresh();return;}
        new AlertDialog.Builder(activity).setTitle("Auto DJ music source").setItems(names.toArray(new String[0]),(d,index)->{java.util.concurrent.ExecutorService worker=java.util.concurrent.Executors.newSingleThreadExecutor();worker.execute(()->{try{
            org.json.JSONObject rules;List<MediaItem> items;
            if(index<lists.length()){rules=lists.getJSONObject(index);items=PhonePlaylists.tracks(activity,rules);}
            else{java.io.File folder=offline.get(index-lists.length());items=RadioOffline.items(folder);rules=RadioOffline.album(folder).getJSONObject("playlist").optJSONObject("rules");if(rules==null)rules=new org.json.JSONObject();else rules.put("smart",true);}
            final org.json.JSONObject selectedRules=rules;
            activity.runOnUiThread(()->{engine.queue(items,selectedRules);if(startAfterLoad&&!items.isEmpty()){host.beforeMainPlay();engine.auto(true);}draw();});
        }catch(Exception error){activity.runOnUiThread(()->{engine.message=error.getMessage();refresh();});}finally{worker.shutdown();}});}).setNegativeButton("Cancel",null).show();
    }catch(Exception error){engine.message=error.getMessage();refresh();}}

    private void send(int side,int target){ExoPlayer cue=engine.decks[side+2];if(cue.getCurrentMediaItem()!=null)engine.load(target,cue.getCurrentMediaItem(),cue.getCurrentPosition());else engine.message="Load a cue track first.";}
    private void padMenu(int side,int pad){new AlertDialog.Builder(activity).setTitle(engine.names[side][pad]).setItems(new String[]{"Replace effect","Rename",engine.loops[side][pad]?"Loop: ON (turn off)":"Loop: OFF (turn on)","Stop","Clear"},(d,index)->{if(index==0)host.effect(side,pad);if(index==1){EditText name=new EditText(activity);name.setText(engine.names[side][pad]);new AlertDialog.Builder(activity).setTitle("Pad name").setView(name).setPositiveButton("Save",(a,b)->{String value=name.getText().toString().trim();if(!value.isEmpty()){engine.rename(side,pad,value);draw();}}).setNegativeButton("Cancel",null).show();}if(index==2)engine.loop(side,pad,!engine.loops[side][pad]);if(index==3)engine.stopPad(side,pad);if(index==4){engine.clearPad(side,pad);draw();}}).show();}
    private void choose(int target){
        LinearLayout view=new LinearLayout(activity);view.setOrientation(VERTICAL);EditText query=new EditText(activity);query.setHint("Search songs");view.addView(query);ListView list=new ListView(activity);view.addView(list,new LayoutParams(-1,dp(320)));
        List<MediaItem> matches=new ArrayList<>();ArrayAdapter<String> adapter=new ArrayAdapter<>(activity,android.R.layout.simple_list_item_1,new ArrayList<>());list.setAdapter(adapter);
        Runnable filter=()->{matches.clear();adapter.clear();for(MediaItem item:stationAndPhoneSongs())if(String.valueOf(item.mediaMetadata.title).toLowerCase(Locale.ROOT).contains(query.getText().toString().toLowerCase(Locale.ROOT))){matches.add(item);adapter.add(String.valueOf(item.mediaMetadata.title));}adapter.notifyDataSetChanged();};
        query.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int s,int c,int a){}public void onTextChanged(CharSequence t,int s,int b,int c){filter.run();}public void afterTextChanged(android.text.Editable e){}});filter.run();
        AlertDialog dialog=new AlertDialog.Builder(activity).setTitle("Load "+(target<2?(target==0?"A":"B"):"Cue Deck")).setView(view).setNegativeButton("Cancel",null).create();list.setOnItemClickListener((p,v,i,id)->{engine.load(target,matches.get(i),0);dialog.dismiss();refresh();});dialog.show();
    }
    private void renderSongs(LinearLayout rows,String query){rows.removeAllViews();int shown=0;for(MediaItem item:(engine.autoQueue.isEmpty()?stationAndPhoneSongs():new ArrayList<>(engine.autoQueue))){if(!String.valueOf(item.mediaMetadata.title).toLowerCase(Locale.ROOT).contains(query.toLowerCase(Locale.ROOT)))continue;if(shown++>=100)break;LinearLayout row=new LinearLayout(activity);Button title=button(String.valueOf(item.mediaMetadata.title),colors.text,()->{engine.load(engine.destination,item,0);refresh();});title.setGravity(Gravity.START|Gravity.CENTER_VERTICAL);title.setOnLongClickListener(v->{v.startDragAndDrop(ClipData.newPlainText("AMP song",item.mediaId),new View.DragShadowBuilder(v),item,0);return true;});row.addView(title,new LayoutParams(0,dp(52),1));for(int d=0;d<2;d++){final int deck=d;row.addView(button(d==0?"A":"B",d==0?DjEngine.AMBER:DjEngine.CYAN,()->{engine.destination(deck);engine.load(deck,item,0);}),new LayoutParams(dp(50),dp(52)));}row.addView(button("⋮",colors.text,()->new AlertDialog.Builder(activity).setTitle("Play within how many songs?").setItems(new String[]{"1","2","3","4","5"},(dialog,n)->{engine.request(item,n+1);refresh();}).show()),new LayoutParams(dp(44),dp(52)));rows.addView(row);}if(shown==0)rows.addView(label("No matching songs. Add phone music in Library.",14));if(shown>100)rows.addView(label("First 100 matches. Refine your search.",12));}
    private void drop(View view,int deck){view.setOnDragListener((v,event)->{if(!(event.getLocalState() instanceof MediaItem))return false;switch(event.getAction()){case DragEvent.ACTION_DRAG_ENTERED:v.setAlpha(.65f);break;case DragEvent.ACTION_DRAG_EXITED:case DragEvent.ACTION_DRAG_ENDED:v.setAlpha(1);break;case DragEvent.ACTION_DROP:engine.load(deck,(MediaItem)event.getLocalState(),0);v.setAlpha(1);refresh();break;default:break;}return true;});}
    private java.util.List<MediaItem> stationAndPhoneSongs(){java.util.List<MediaItem> items=new java.util.ArrayList<>(LibraryStore.read(activity));items.addAll(RadioOffline.all(activity));return items;}
    private void cues(int deck){MediaItem item=engine.decks[deck].getCurrentMediaItem();if(item==null)return;String[] options=new String[9];for(int n=0;n<8;n++){long value=PhoneCues.seconds(activity,item,n);options[n]="Cue "+(n+1)+" · "+(value>=0?MainActivity.time(value):"empty");}options[8]="Save current position to cue…";new AlertDialog.Builder(activity).setTitle("Hot cues · phone edits stay local").setItems(options,(d,n)->{if(n<8){long value=PhoneCues.seconds(activity,item,n);if(value>=0)engine.seek(deck,value);}else new AlertDialog.Builder(activity).setTitle("Save cue slot").setItems(new String[]{"1","2","3","4","5","6","7","8"},(a,slot)->PhoneCues.save(activity,item,slot,engine.decks[deck].getCurrentPosition())).show();}).setNegativeButton("Close",null).show();}
    private void sync(int deck){MediaItem own=engine.decks[deck].getCurrentMediaItem(),other=engine.decks[1-deck].getCurrentMediaItem();if(own==null||other==null){engine.message="Load both tracks first.";return;}double a=PhonePlaylists.bpm(activity,own),b=PhonePlaylists.bpm(activity,other);if(a<=0||b<=0){engine.message="Set BPM in Library track details before tempo sync.";return;}engine.decks[deck].setPlaybackSpeed((float)SharedDjLogic.tempoRate(b,engine.decks[1-deck].getPlaybackParameters().speed,a,.16));engine.message="Tempo matched within ±16%; beat phase is manual.";}
    void refresh(){for(int i=0;i<4;i++)if(info[i]!=null){ExoPlayer deck=engine.decks[i];long duration=deck.getDuration();info[i].setText(engine.title(i)+"\n"+MainActivity.time(deck.getCurrentPosition())+" / "+(duration>0?MainActivity.time(duration):"--:--")+(engine.staged[i]!=null?" · staged replacement":""));if(timelines[i]!=null&&!timelines[i].isPressed())timelines[i].setProgress(duration>0?(int)(deck.getCurrentPosition()*1000/duration):0);}if(status!=null)status.setText(engine.message);if(autoToggle!=null){updatingAuto=true;autoToggle.setChecked(engine.auto);updatingAuto=false;}if(fader!=null&&!fader.isPressed())fader.setProgress((int)(engine.fader*1000));if(mix!=null)mix.setText(engine.mixing()?"Cancel crossfade":"Mix next · "+engine.fadeSeconds+"s");}
    @Override protected void onAttachedToWindow(){super.onAttachedToWindow();timer.post(tick);}
    @Override protected void onDetachedFromWindow(){timer.removeCallbacks(tick);super.onDetachedFromWindow();}
    void redraw(){draw();}
    @android.annotation.SuppressLint("AppCompatCustomView") // Main player retains its native Activity/theme; this pad is not used in the AppCompat Cast picker.
    private static final class SquarePad extends Button {
        SquarePad(Context context){super(context);}
        @Override protected void onMeasure(int widthSpec,int heightSpec){super.onMeasure(widthSpec,heightSpec);int width=getMeasuredWidth();if(width>0)setMeasuredDimension(width,width);}
    }
    private static final class Jog extends View {
        private final Paint paint=new Paint(3);private final int color;private final java.util.function.LongConsumer seek;private float last;
        Jog(Context c,int color,java.util.function.LongConsumer seek){super(c);this.color=color;this.seek=seek;setContentDescription("Jog wheel: drag to seek");}
        @Override protected void onDraw(Canvas canvas){super.onDraw(canvas);float x=getWidth()/2f,y=getHeight()/2f,r=Math.min(x,y)-6;paint.setStyle(Paint.Style.FILL);paint.setColor(Color.rgb(21,24,30));canvas.drawCircle(x,y,r,paint);paint.setStyle(Paint.Style.STROKE);paint.setStrokeWidth(3);paint.setColor(color);canvas.drawCircle(x,y,r,paint);paint.setStrokeWidth(1);paint.setColor(Color.argb(100,140,150,165));for(float ring=.43f;ring<.97f;ring+=.055f)canvas.drawCircle(x,y,r*ring,paint);paint.setStyle(Paint.Style.FILL);paint.setColor(color);canvas.drawCircle(x,y,r*.36f,paint);paint.setColor(Color.rgb(20,25,34));canvas.drawCircle(x,y,r*.29f,paint);paint.setStyle(Paint.Style.STROKE);paint.setColor(color);paint.setStrokeWidth(3);canvas.drawLine(x+r*.4f,y,x+r*.94f,y,paint);paint.setStyle(Paint.Style.FILL);paint.setColor(color);paint.setTextAlign(Paint.Align.CENTER);paint.setTextSize(r*.38f);canvas.drawText("ϟ",x,y+r*.13f,paint);}

        @Override public boolean onTouchEvent(android.view.MotionEvent event){if(event.getAction()==android.view.MotionEvent.ACTION_DOWN){last=event.getX();getParent().requestDisallowInterceptTouchEvent(true);return true;}if(event.getAction()==android.view.MotionEvent.ACTION_MOVE){float delta=event.getX()-last;last=event.getX();seek.accept((long)(delta*30));return true;}if(event.getAction()==android.view.MotionEvent.ACTION_UP||event.getAction()==android.view.MotionEvent.ACTION_CANCEL){getParent().requestDisallowInterceptTouchEvent(false);performClick();return true;}return true;}
        @Override public boolean performClick(){super.performClick();return true;}
    }
}
