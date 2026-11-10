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
    private Button mix,autoToggle;
    private int visibleDeck;
    private final android.os.Handler timer=new android.os.Handler(android.os.Looper.getMainLooper());
    private final Runnable tick=new Runnable(){public void run(){refresh();timer.postDelayed(this,250);}};
    DjPanel(Activity activity,Appearance colors,DjEngine engine,Host host){super(activity);this.activity=activity;this.colors=colors;this.engine=engine;this.host=host;setOrientation(VERTICAL);draw();}
    private int dp(int n){return (int)(n*getResources().getDisplayMetrics().density);}
    private TextView label(String value,int size){TextView v=new TextView(activity);v.setText(value);v.setTextSize(size);v.setTextColor(colors.text);v.setPadding(dp(4),dp(6),dp(4),dp(6));return v;}
    private Button button(String text,int color,Runnable action){return decorate(new Button(activity),text,color,action);}
    private int readable(int color){if(Color.red(colors.background)>200){if(color==DjEngine.AMBER)return 0xff8a5c00;if(color==DjEngine.CYAN)return 0xff007b8a;}return color;}
    private Button decorate(Button b,String text,int color,Runnable action){b.setText(text);b.setAllCaps(false);b.setTextColor(readable(color));b.setTextSize(13);b.setMinWidth(0);b.setMinimumWidth(0);b.setPadding(dp(5),0,dp(5),0);b.setMinHeight(dp(48));b.setBackground(Chrome.button(colors,false));b.setContentDescription(text);b.setOnClickListener(v->{action.run();refresh();});return b;}
    private LinearLayout card(LinearLayout parent){LinearLayout v=new LinearLayout(activity);v.setOrientation(VERTICAL);v.setPadding(dp(8),dp(8),dp(8),dp(8));v.setBackground(Chrome.panel(colors,false));LayoutParams p=new LayoutParams(-1,-2);p.setMargins(0,dp(8),0,0);parent.addView(v,p);return v;}
    private void row(LinearLayout parent,Button...buttons){LinearLayout row=new LinearLayout(activity);for(Button b:buttons){LayoutParams p=new LayoutParams(0,dp(50),1);p.setMargins(dp(2),dp(2),dp(2),dp(2));row.addView(b,p);}parent.addView(row);}
    private SeekBar slider(LinearLayout parent,int max,int value,java.util.function.IntConsumer changed){SeekBar bar=new SeekBar(activity);bar.setMax(max);bar.setProgress(value);bar.setMinimumHeight(dp(48));bar.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){}public void onStopTrackingTouch(SeekBar b){}public void onProgressChanged(SeekBar b,int p,boolean user){if(user)changed.accept(p);}});parent.addView(bar,new LayoutParams(-1,dp(48)));return bar;}
    private void draw(){
        removeAllViews();Arrays.fill(info,null);Arrays.fill(timelines,null);
        LinearLayout toolbar=card(this);toolbar.addView(label("AMP DJ · Phone mix",22));
        row(toolbar,button("Load → A",DjEngine.AMBER,()->{engine.destination(0);choose(0);}),button("Load → B",DjEngine.CYAN,()->{engine.destination(1);choose(1);}));
        autoToggle=button(engine.auto?"Stop Auto DJ":"Start Auto DJ",colors.accent,()->{if(!engine.auto)host.beforeMainPlay();engine.auto(!engine.auto);draw();});row(toolbar,button("Load playlist",colors.accent,this::chooseAutoSource),autoToggle,button("EQ",colors.accent,host::equalizer));
        toolbar.addView(label("Prepared tracks load paused. Playing decks stage replacements.",13));
        status=label(engine.message,14);toolbar.addView(status);
        boolean wide=getResources().getConfiguration().screenWidthDp>=700;
        LinearLayout mains=new LinearLayout(activity);mains.setOrientation(wide?HORIZONTAL:VERTICAL);addView(mains);
        if(!wide){row(mains,button("DECK A",DjEngine.AMBER,()->{visibleDeck=0;draw();}),button("DECK B",DjEngine.CYAN,()->{visibleDeck=1;draw();}));}
        for(int deck=0;deck<2;deck++){if(!wide&&deck!=visibleDeck)continue;LinearLayout column=new LinearLayout(activity);column.setOrientation(VERTICAL);mains.addView(column,new LayoutParams(wide?0:-1,-2,wide?1:0));deck(column,deck);}
        LinearLayout center=card(this);center.addView(label("Master volume",15));slider(center,100,(int)(engine.master*100),p->{engine.master=p/100f;engine.setFader(engine.fader);});center.addView(label("Crossfader · A ↔ B",18));
        fader=slider(center,1000,(int)(engine.fader*1000),p->{if(engine.auto)engine.auto(false);engine.cancelMix();engine.setFader(p/1000f);});fader.setContentDescription("DJ crossfader");
        mix=button("Auto Crossfade · 8 seconds",colors.accent,()->{if(engine.auto)engine.auto(false);host.beforeMainPlay();engine.mixNext();});center.addView(mix);
        center.addView(label("Equal-power transition. Beat-phase matching and reverse-audio scratching require the desktop DJ engine.",12));
        LinearLayout aux=new LinearLayout(activity);aux.setOrientation(wide?HORIZONTAL:VERTICAL);addView(aux);
        for(int side=0;side<2;side++){LinearLayout column=new LinearLayout(activity);column.setOrientation(VERTICAL);aux.addView(column,new LayoutParams(wide?0:-1,-2,wide?1:0));side(column,side);}
        LinearLayout library=card(this);library.addView(label("DJ Library",20));library.addView(button("All phone music",colors.accent,()->{engine.queue(new ArrayList<>());draw();}));
        EditText search=new EditText(activity);search.setSingleLine(true);search.setHint("Search phone music");search.setTextColor(colors.text);search.setHintTextColor(colors.muted);search.setContentDescription("Search DJ music");library.addView(search);
        LinearLayout songs=new LinearLayout(activity);songs.setOrientation(VERTICAL);library.addView(songs);
        Runnable filter=()->renderSongs(songs,search.getText().toString());
        search.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int s,int c,int a){}public void onTextChanged(CharSequence t,int s,int b,int c){filter.run();}public void afterTextChanged(android.text.Editable e){}});filter.run();refresh();
    }
    private void deck(LinearLayout parent,int index){
        int color=index==0?DjEngine.AMBER:DjEngine.CYAN;LinearLayout card=card(parent);TextView heading=label("DECK "+(index==0?"A":"B"),20);heading.setTextColor(readable(color));card.addView(heading);transport(card,index,color);
        Jog jog=new Jog(activity,color,delta->{engine.cancelMix();engine.seek(index,engine.decks[index].getCurrentPosition()+delta);});card.addView(jog,new LayoutParams(-1,dp(110)));
        card.addView(label("Jog to seek · pitch ±16%",12));slider(card,32,16,p->engine.decks[index].setPlaybackSpeed(1+(p-16)/100f));
        row(card,button("Sync tempo",color,()->sync(index)),button("Hot cues",color,()->cues(index)),button("Loop",color,()->{ExoPlayer deck=engine.decks[index];deck.setRepeatMode(deck.getRepeatMode()==Player.REPEAT_MODE_OFF?Player.REPEAT_MODE_ONE:Player.REPEAT_MODE_OFF);engine.message=deck.getRepeatMode()==Player.REPEAT_MODE_ONE?"Track loop enabled":"Track loop disabled";}));
        drop(card,index);
    }
    private void transport(LinearLayout card,int index,int color){
        info[index]=label("No track loaded",15);card.addView(info[index]);
        timelines[index]=slider(card,1000,0,p->{long duration=engine.decks[index].getDuration();if(duration>0)engine.seek(index,duration*p/1000);});timelines[index].setContentDescription("Seek "+(index<2?"deck "+(index==0?"A":"B"):"cue "+(index-1)));
        row(card,button("Load",color,()->choose(index)),button("Play / Pause",color,()->{if(engine.active(index))engine.decks[index].pause();else{if(index<2)host.beforeMainPlay();engine.play(index);}}),button("Stop",color,()->{if(index<2&&engine.auto)engine.auto(false);engine.stop(index);}));
        row(card,button("−1s",color,()->engine.seek(index,engine.decks[index].getCurrentPosition()-1000)),button("+1s",color,()->engine.seek(index,engine.decks[index].getCurrentPosition()+1000)),button("Apply staged",color,()->engine.apply(index)));
    }
    private void side(LinearLayout parent,int side){
        int color=side==0?DjEngine.AMBER:DjEngine.CYAN;LinearLayout card=card(parent);
        card.addView(button((engine.collapsed[side]?"▸ ":"▾ ")+(side==0?"Left":"Right")+" side deck",color,()->{engine.collapse(side);draw();}));if(engine.collapsed[side])return;
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
    private void chooseAutoSource(){try{org.json.JSONArray lists=PhonePlaylists.read(activity);String[] names=new String[lists.length()];for(int i=0;i<names.length;i++)names[i]=lists.getJSONObject(i).optString("name");if(names.length==0){engine.message="Create a playlist in Library first.";refresh();return;}new AlertDialog.Builder(activity).setTitle("Auto DJ music source").setItems(names,(d,index)->{java.util.concurrent.ExecutorService worker=java.util.concurrent.Executors.newSingleThreadExecutor();worker.execute(()->{try{List<MediaItem> items=PhonePlaylists.tracks(activity,lists.getJSONObject(index));activity.runOnUiThread(()->{engine.queue(items);draw();});}catch(Exception error){activity.runOnUiThread(()->{engine.message=error.getMessage();refresh();});}finally{worker.shutdown();}});}).setNegativeButton("Cancel",null).show();}catch(Exception error){engine.message=error.getMessage();refresh();}}
    private void send(int side,int target){ExoPlayer cue=engine.decks[side+2];if(cue.getCurrentMediaItem()!=null)engine.load(target,cue.getCurrentMediaItem(),cue.getCurrentPosition());else engine.message="Load a cue track first.";}
    private void padMenu(int side,int pad){new AlertDialog.Builder(activity).setTitle(engine.names[side][pad]).setItems(new String[]{"Replace effect","Rename",engine.loops[side][pad]?"Loop: ON (turn off)":"Loop: OFF (turn on)","Stop","Clear"},(d,index)->{if(index==0)host.effect(side,pad);if(index==1){EditText name=new EditText(activity);name.setText(engine.names[side][pad]);new AlertDialog.Builder(activity).setTitle("Pad name").setView(name).setPositiveButton("Save",(a,b)->{String value=name.getText().toString().trim();if(!value.isEmpty()){engine.names[side][pad]=value;activity.getSharedPreferences("dj",0).edit().putString("name-"+side+"-"+pad,value).apply();draw();}}).setNegativeButton("Cancel",null).show();}if(index==2)engine.loop(side,pad,!engine.loops[side][pad]);if(index==3)engine.stopPad(side,pad);if(index==4){engine.clearPad(side,pad);draw();}}).show();}
    private void choose(int target){
        LinearLayout view=new LinearLayout(activity);view.setOrientation(VERTICAL);EditText query=new EditText(activity);query.setHint("Search songs");view.addView(query);ListView list=new ListView(activity);view.addView(list,new LayoutParams(-1,dp(320)));
        List<MediaItem> matches=new ArrayList<>();ArrayAdapter<String> adapter=new ArrayAdapter<>(activity,android.R.layout.simple_list_item_1,new ArrayList<>());list.setAdapter(adapter);
        Runnable filter=()->{matches.clear();adapter.clear();for(MediaItem item:LibraryStore.read(activity))if(String.valueOf(item.mediaMetadata.title).toLowerCase(Locale.ROOT).contains(query.getText().toString().toLowerCase(Locale.ROOT))){matches.add(item);adapter.add(String.valueOf(item.mediaMetadata.title));}adapter.notifyDataSetChanged();};
        query.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int s,int c,int a){}public void onTextChanged(CharSequence t,int s,int b,int c){filter.run();}public void afterTextChanged(android.text.Editable e){}});filter.run();
        AlertDialog dialog=new AlertDialog.Builder(activity).setTitle("Load "+(target<2?(target==0?"A":"B"):"Cue Deck")).setView(view).setNegativeButton("Cancel",null).create();list.setOnItemClickListener((p,v,i,id)->{engine.load(target,matches.get(i),0);dialog.dismiss();refresh();});dialog.show();
    }
    private void renderSongs(LinearLayout rows,String query){rows.removeAllViews();int shown=0;for(MediaItem item:(engine.autoQueue.isEmpty()?LibraryStore.read(activity):new ArrayList<>(engine.autoQueue))){if(!String.valueOf(item.mediaMetadata.title).toLowerCase(Locale.ROOT).contains(query.toLowerCase(Locale.ROOT)))continue;if(shown++>=100)break;LinearLayout row=new LinearLayout(activity);Button title=button(String.valueOf(item.mediaMetadata.title),colors.text,()->{engine.load(engine.destination,item,0);refresh();});title.setGravity(Gravity.START|Gravity.CENTER_VERTICAL);title.setOnLongClickListener(v->{v.startDragAndDrop(ClipData.newPlainText("AMP song",item.mediaId),new View.DragShadowBuilder(v),item,0);return true;});row.addView(title,new LayoutParams(0,dp(52),1));for(int d=0;d<2;d++){final int deck=d;row.addView(button(d==0?"A":"B",d==0?DjEngine.AMBER:DjEngine.CYAN,()->{engine.destination(deck);engine.load(deck,item,0);}),new LayoutParams(dp(50),dp(52)));}rows.addView(row);}if(shown==0)rows.addView(label("No matching songs. Add phone music in Library.",14));if(shown>100)rows.addView(label("First 100 matches. Refine your search.",12));}
    private void drop(View view,int deck){view.setOnDragListener((v,event)->{if(!(event.getLocalState() instanceof MediaItem))return false;switch(event.getAction()){case DragEvent.ACTION_DRAG_ENTERED:v.setAlpha(.65f);break;case DragEvent.ACTION_DRAG_EXITED:case DragEvent.ACTION_DRAG_ENDED:v.setAlpha(1);break;case DragEvent.ACTION_DROP:engine.load(deck,(MediaItem)event.getLocalState(),0);v.setAlpha(1);refresh();break;default:break;}return true;});}
    private void cues(int deck){MediaItem item=engine.decks[deck].getCurrentMediaItem();if(item==null)return;SharedPreferences prefs=activity.getSharedPreferences("phone-cues",0);String prefix=item.mediaId+":";String[] options=new String[9];for(int n=0;n<8;n++)options[n]="Cue "+(n+1)+" · "+(prefs.contains(prefix+n)?MainActivity.time(prefs.getLong(prefix+n,0)):"empty");options[8]="Save current position to cue…";new AlertDialog.Builder(activity).setTitle("Local cue overrides").setItems(options,(d,n)->{if(n<8){if(prefs.contains(prefix+n))engine.seek(deck,prefs.getLong(prefix+n,0));}else new AlertDialog.Builder(activity).setTitle("Save cue slot").setItems(new String[]{"1","2","3","4","5","6","7","8"},(a,slot)->prefs.edit().putLong(prefix+slot,engine.decks[deck].getCurrentPosition()).apply()).show();}).setNegativeButton("Close",null).show();}
    private void sync(int deck){MediaItem own=engine.decks[deck].getCurrentMediaItem(),other=engine.decks[1-deck].getCurrentMediaItem();if(own==null||other==null){engine.message="Load both tracks first.";return;}double a=PhonePlaylists.bpm(activity,own),b=PhonePlaylists.bpm(activity,other);if(a<=0||b<=0){engine.message="Set BPM in Library track details before tempo sync.";return;}engine.decks[deck].setPlaybackSpeed((float)Math.max(.84,Math.min(1.16,b*engine.decks[1-deck].getPlaybackParameters().speed/a)));engine.message="Tempo matched within ±16%; beat phase is manual.";}
    void refresh(){for(int i=0;i<4;i++)if(info[i]!=null){ExoPlayer deck=engine.decks[i];long duration=deck.getDuration();info[i].setText(engine.title(i)+"\n"+MainActivity.time(deck.getCurrentPosition())+" / "+(duration>0?MainActivity.time(duration):"--:--")+(engine.staged[i]!=null?" · staged replacement":""));if(timelines[i]!=null&&!timelines[i].isPressed())timelines[i].setProgress(duration>0?(int)(deck.getCurrentPosition()*1000/duration):0);}if(status!=null)status.setText(engine.message);if(autoToggle!=null)autoToggle.setText(engine.auto?"Stop Auto DJ":"Start Auto DJ");if(fader!=null&&!fader.isPressed())fader.setProgress((int)(engine.fader*1000));if(mix!=null)mix.setText(engine.mixing()?"Cancel crossfade":"Auto Crossfade · 8 seconds");}
    @Override protected void onAttachedToWindow(){super.onAttachedToWindow();timer.post(tick);}
    @Override protected void onDetachedFromWindow(){timer.removeCallbacks(tick);super.onDetachedFromWindow();}
    void redraw(){draw();}
    private static final class SquarePad extends Button {
        SquarePad(Context context){super(context);}
        @Override protected void onMeasure(int widthSpec,int heightSpec){super.onMeasure(widthSpec,heightSpec);int width=getMeasuredWidth();if(width>0)setMeasuredDimension(width,width);}
    }
    private static final class Jog extends View {
        private final Paint paint=new Paint(3);private final int color;private final java.util.function.LongConsumer seek;private float last;
        Jog(Context c,int color,java.util.function.LongConsumer seek){super(c);this.color=color;this.seek=seek;setContentDescription("Jog wheel: drag to seek");}
        @Override protected void onDraw(Canvas canvas){super.onDraw(canvas);float x=getWidth()/2f,y=getHeight()/2f,r=Math.min(x,y)-6;paint.setColor(color);canvas.drawCircle(x,y,r,paint);paint.setColor(Color.rgb(20,25,34));canvas.drawCircle(x,y,r*.38f,paint);paint.setColor(Color.WHITE);paint.setTextSize(30);canvas.drawText("ϟ",x-9,y+10,paint);}
        @Override public boolean onTouchEvent(android.view.MotionEvent event){if(event.getAction()==android.view.MotionEvent.ACTION_DOWN){last=event.getX();getParent().requestDisallowInterceptTouchEvent(true);return true;}if(event.getAction()==android.view.MotionEvent.ACTION_MOVE){float delta=event.getX()-last;last=event.getX();seek.accept((long)(delta*30));return true;}if(event.getAction()==android.view.MotionEvent.ACTION_UP||event.getAction()==android.view.MotionEvent.ACTION_CANCEL){getParent().requestDisallowInterceptTouchEvent(false);performClick();return true;}return true;}
        @Override public boolean performClick(){super.performClick();return true;}
    }
}
