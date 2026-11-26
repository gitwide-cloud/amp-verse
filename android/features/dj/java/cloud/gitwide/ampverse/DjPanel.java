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
    private final Button[] deckSelectors=new Button[2];
    private final ImageView[] covers=new ImageView[4];
    private final String[] coverIds=new String[4];
    private final byte[][] coverData=new byte[4][];
    private TextView secondaryInfo;
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
    private Button decorate(Button b,String text,int color,Runnable action){b.setText(text);b.setAllCaps(false);b.setTextColor(readable(color));b.setTextSize(13);b.setMinWidth(0);b.setMinimumWidth(0);b.setPadding(dp(5),0,dp(5),0);b.setMinHeight(dp(48));b.setMinimumHeight(dp(48));b.setBackground(Chrome.button(colors,false));b.setContentDescription(text);b.setOnClickListener(v->{action.run();refresh();});return b;}
    private LinearLayout card(LinearLayout parent){LinearLayout v=new LinearLayout(activity);v.setOrientation(VERTICAL);v.setPadding(dp(10),dp(10),dp(10),dp(10));v.setBackground(Chrome.panel(colors,false));LayoutParams p=new LayoutParams(-1,-2);p.setMargins(0,dp(8),0,0);parent.addView(v,p);return v;}
    private void row(LinearLayout parent,Button...buttons){LinearLayout row=new LinearLayout(activity);for(Button b:buttons){LayoutParams p=new LayoutParams(0,-2,1);p.setMargins(dp(2),dp(2),dp(2),dp(2));row.addView(b,p);}parent.addView(row);}
    private SeekBar slider(LinearLayout parent,int max,int value,java.util.function.IntConsumer changed){SeekBar bar=new SeekBar(activity);Chrome.slider(bar,colors.accent);bar.setMax(max);bar.setProgress(value);bar.setMinimumHeight(dp(48));bar.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){}public void onStopTrackingTouch(SeekBar b){}public void onProgressChanged(SeekBar b,int p,boolean user){if(user)changed.accept(p);}});parent.addView(bar,new LayoutParams(-1,dp(48)));return bar;}
    private void draw(){
        removeAllViews();Arrays.fill(info,null);Arrays.fill(timelines,null);Arrays.fill(covers,null);Arrays.fill(coverIds,null);Arrays.fill(coverData,null);Arrays.fill(deckSelectors,null);secondaryInfo=null;
        boolean wide=getResources().getConfiguration().screenWidthDp>=700;
        LinearLayout toolbar=card(this);toolbar.setBackground(Chrome.highlighted(colors,colors.accent));
        LinearLayout quick=new LinearLayout(activity);quick.setGravity(Gravity.CENTER_VERTICAL);toolbar.addView(quick);
        Button source=button("Choose playlist  ›",colors.accent,this::chooseAutoSource);source.setCompoundDrawablesWithIntrinsicBounds(new CardIcon(activity,"folder",colors.accent),null,null,null);source.setTag("dj-choose-playlist");quick.addView(source,new LayoutParams(0,dp(wide?48:56),1));
        autoToggle=new Switch(activity);autoToggle.setText("Auto Mix");autoToggle.setTextColor(colors.text);Chrome.toggle(autoToggle,colors);autoToggle.setMinHeight(dp(48));autoToggle.setPadding(dp(8),0,dp(4),0);autoToggle.setContentDescription("Phone Auto Mix");autoToggle.setChecked(engine.auto);autoToggle.setOnCheckedChangeListener((v,on)->{if(updatingAuto)return;if(on&&engine.autoQueue.isEmpty()){updatingAuto=true;autoToggle.setChecked(false);updatingAuto=false;chooseAutoSource(true);return;}if(on)host.beforeMainPlay();engine.auto(on);refresh();});quick.addView(autoToggle,new LayoutParams(-2,dp(wide?48:56)));
        status=label(engine.message,13);status.setTextColor(colors.muted);status.setAccessibilityLiveRegion(View.ACCESSIBILITY_LIVE_REGION_POLITE);if(wide){toolbar.setOrientation(HORIZONTAL);toolbar.setPadding(dp(8),dp(4),dp(8),dp(4));quick.setLayoutParams(new LayoutParams(0,-2,2));status.setMaxLines(2);toolbar.addView(status,new LayoutParams(0,-2,1));}else toolbar.addView(status);
        LinearLayout mains=new LinearLayout(activity);mains.setOrientation(wide?HORIZONTAL:VERTICAL);addView(mains);
        if(!wide){Button a=button("DECK A",DjEngine.AMBER,()->{visibleDeck=0;engine.destination(0);draw();}),b=button("DECK B",DjEngine.CYAN,()->{visibleDeck=1;engine.destination(1);draw();});deckSelectors[0]=a;deckSelectors[1]=b;Button selected=visibleDeck==0?a:b;selected.setSelected(true);selected.setBackground(Chrome.highlighted(colors,visibleDeck==0?DjEngine.AMBER:DjEngine.CYAN));a.setTag("deck-select-a");b.setTag("deck-select-b");a.setMaxLines(2);b.setMaxLines(2);row(mains,a,b);}
        for(int deck=0;deck<2;deck++){if(!wide&&deck!=visibleDeck)continue;LinearLayout column=new LinearLayout(activity);column.setOrientation(VERTICAL);mains.addView(column,new LayoutParams(wide?0:-1,-2,wide?1:0));deck(column,deck);}
        LinearLayout center;
        pinnedMixer.removeAllViews();
        if(wide){center=card(this);center.addView(label("Crossfader · A ↔ B",18));}
        else{center=pinnedMixer;TextView a=label("A",14);a.setTextColor(readable(DjEngine.AMBER));center.addView(a);}
        fader=slider(center,1000,(int)(engine.fader*1000),p->{if(engine.auto)engine.auto(false);engine.cancelMix();engine.setFader(p/1000f);});fader.setContentDescription("DJ crossfader");
        if(!wide){fader.setLayoutParams(new LayoutParams(0,dp(48),1));TextView b=label("B",14);b.setTextColor(readable(DjEngine.CYAN));center.addView(b);}
        mix=button("Mix next",colors.accent,()->{host.beforeMainPlay();engine.mixNext();});mix.setBackground(Chrome.button(colors,true));mix.setTextColor(Color.BLACK);center.addView(mix,new LayoutParams(wide?-1:dp(108),-2));
        Button settings=button(wide?"Mixer settings":"⋮",colors.text,this::mixerSettings);settings.setContentDescription("DJ mixer settings");center.addView(settings,new LayoutParams(wide?-1:dp(48),-2));
        if(wide){removeView(center);mains.addView(center,1,new LayoutParams(0,-2,.7f));}
        int auxiliaryStart=getChildCount();
        LinearLayout aux=new LinearLayout(activity);aux.setOrientation(VERTICAL);addView(aux);
        hotCueBank(aux); samplerBank(aux);
        for(int side=0;side<2;side++) side(aux,side);
        UiSections.fold(this,colors,"Hot cues, sampler & Cue Decks",auxiliaryStart);
        LinearLayout library=card(this);library.addView(label("DJ Library",20));library.addView(button("Browse all music",colors.text,()->{engine.queue(new ArrayList<>());draw();}));
        EditText search=new EditText(activity);search.setSingleLine(true);search.setHint("Search phone music");search.setTextColor(colors.text);search.setHintTextColor(colors.muted);search.setContentDescription("Search DJ music");library.addView(search);
        LinearLayout songs=new LinearLayout(activity);songs.setOrientation(VERTICAL);library.addView(songs);
        Runnable filter=()->renderSongs(songs,search.getText().toString());
        search.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int s,int c,int a){}public void onTextChanged(CharSequence t,int s,int b,int c){filter.run();}public void afterTextChanged(android.text.Editable e){}});filter.run();refresh();
    }
    private void mixerSettings(){LinearLayout form=new LinearLayout(activity);form.setOrientation(VERTICAL);form.setPadding(dp(12),dp(8),dp(12),dp(8));form.setBackground(Chrome.panel(colors,false));form.addView(label("Master volume",16));slider(form,100,(int)(engine.master*100),p->{engine.master=p/100f;engine.setFader(engine.fader);});TextView timing=label("Crossfade · "+engine.fadeSeconds+" seconds",16);form.addView(timing);row(form,button("−1s",colors.text,()->{engine.fadeSeconds(engine.fadeSeconds-1);timing.setText("Crossfade · "+engine.fadeSeconds+" seconds");refresh();}),button("+1s",colors.text,()->{engine.fadeSeconds(engine.fadeSeconds+1);timing.setText("Crossfade · "+engine.fadeSeconds+" seconds");refresh();}));form.addView(button("Equalizer",colors.text,host::equalizer));ScrollView scroll=new ScrollView(activity);scroll.addView(form);new AlertDialog.Builder(activity).setTitle("DJ mixer").setView(scroll).setPositiveButton("Done",null).show();}
    private void deck(LinearLayout parent,int index){
        int color=index==0?DjEngine.AMBER:DjEngine.CYAN;LinearLayout main=card(parent);main.setBackground(Chrome.highlighted(colors,color));boolean compact=getResources().getConfiguration().screenWidthDp>=700;if(compact)main.setPadding(dp(8),dp(6),dp(8),dp(6));TextView heading=label("DECK "+(index==0?"A":"B")+" · PHONE",compact?16:18);heading.setTag("deck-heading-"+index);heading.setTypeface(null,Typeface.BOLD);heading.setTextColor(readable(color));main.addView(heading);transport(main,index,color);drop(main,index);
        if(getResources().getConfiguration().screenWidthDp<700){
            final int other=1-index;LinearLayout secondary=card(parent);secondary.setOrientation(HORIZONTAL);secondary.setGravity(Gravity.CENTER_VERTICAL);secondaryInfo=label("",14);secondaryInfo.setTextColor(readable(other==0?DjEngine.AMBER:DjEngine.CYAN));secondary.addView(secondaryInfo,new LayoutParams(0,-2,1));Button stage=button("Load / stage  ›",colors.text,()->choose(other));secondary.addView(stage,new LayoutParams(-2,dp(52)));
        }
        LinearLayout vinyl=card(parent);boolean surround=getResources().getConfiguration().screenWidthDp>=360&&getResources().getConfiguration().fontScale<=1.25f;
        LinearLayout left=new LinearLayout(activity),right=new LinearLayout(activity);left.setOrientation(VERTICAL);right.setOrientation(VERTICAL);
        Button cue=button("Hot cues",color,()->cues(index)),loop=button("Loop",color,()->{ExoPlayer deck=engine.decks[index];deck.setRepeatMode(deck.getRepeatMode()==Player.REPEAT_MODE_OFF?Player.REPEAT_MODE_ONE:Player.REPEAT_MODE_OFF);engine.message=deck.getRepeatMode()==Player.REPEAT_MODE_ONE?"Track loop enabled":"Track loop disabled";}),sync=button("Sync tempo",color,()->sync(index));
        Button staged=button("Seek & staged",color,()->seekAndStage(index)),settings=button("Deck settings",color,()->deckSettings(index)),eq=button("EQ",color,host::equalizer);
        Jog jog=new Jog(activity,color,index);
        if(surround){vinyl.setOrientation(HORIZONTAL);vinyl.setGravity(Gravity.CENTER_VERTICAL);for(Button control:new Button[]{cue,loop,sync}){LayoutParams slot=new LayoutParams(-1,dp(58));slot.setMargins(0,dp(3),0,dp(3));left.addView(control,slot);}for(Button control:new Button[]{staged,settings,eq}){LayoutParams slot=new LayoutParams(-1,dp(58));slot.setMargins(0,dp(3),0,dp(3));right.addView(control,slot);}vinyl.addView(left,new LayoutParams(0,-2,1));LayoutParams disc=new LayoutParams(0,dp(190),1.4f);disc.setMargins(dp(5),0,dp(5),0);vinyl.addView(jog,disc);vinyl.addView(right,new LayoutParams(0,-2,1));}
        else{vinyl.addView(jog,new LayoutParams(-1,dp(190)));row(vinyl,cue,loop,sync);row(vinyl,staged,settings,eq);}
    }
    private void deckSettings(int index){LinearLayout form=new LinearLayout(activity);form.setOrientation(VERTICAL);form.setPadding(dp(12),dp(12),dp(12),dp(12));form.setBackground(Chrome.panel(colors,false));form.addView(label("Pitch ±16% · tempo adjustment",16));slider(form,32,Math.round((engine.decks[index].getPlaybackParameters().speed-1)*100)+16,p->engine.decks[index].setPlaybackSpeed(1+(p-16)/100f));form.addView(button("Equalizer",colors.text,host::equalizer));new AlertDialog.Builder(activity).setTitle("Deck "+(index==0?"A":"B")+" settings").setView(form).setPositiveButton("Done",null).show();}
    private void seekAndStage(int index){LinearLayout form=new LinearLayout(activity);form.setOrientation(VERTICAL);form.setPadding(dp(12),dp(8),dp(12),dp(8));form.addView(label("Active tracks keep playing when a replacement is staged. Stop the deck before applying it.",14));row(form,button("−1s",colors.text,()->engine.seek(index,engine.decks[index].getCurrentPosition()-1000)),button("+1s",colors.text,()->engine.seek(index,engine.decks[index].getCurrentPosition()+1000)));form.addView(button("Load / stage track",colors.text,()->choose(index)));form.addView(button("Apply staged",colors.accent,()->engine.apply(index)));new AlertDialog.Builder(activity).setTitle("Seek & staged track").setView(form).setPositiveButton("Done",null).show();}
    private void transport(LinearLayout card,int index,int color){
        if(index>=2)card.addView(new Jog(activity,color,index),new LayoutParams(-1,dp(110)));
        LinearLayout identity=new LinearLayout(activity);identity.setGravity(Gravity.CENTER_VERTICAL);covers[index]=new ImageView(activity);covers[index].setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);identity.addView(covers[index],new LayoutParams(dp(index<2&&getResources().getConfiguration().screenWidthDp<700?72:48),dp(index<2&&getResources().getConfiguration().screenWidthDp<700?72:48)));info[index]=label("No track loaded",index<2&&getResources().getConfiguration().screenWidthDp<700?18:14);info[index].setMaxLines(getResources().getConfiguration().screenWidthDp>=700?3:5);info[index].setEllipsize(android.text.TextUtils.TruncateAt.END);info[index].setTypeface(null,Typeface.BOLD);info[index].setPadding(dp(10),dp(4),0,dp(4));identity.addView(info[index],new LayoutParams(0,-2,1));card.addView(identity);
        timelines[index]=slider(card,1000,0,p->{long duration=engine.decks[index].getDuration();if(duration>0)engine.seek(index,duration*p/1000);});Chrome.slider(timelines[index],readable(color));timelines[index].setContentDescription("Seek "+(index<2?"deck "+(index==0?"A":"B"):"cue "+(index-1)));boolean compactTransport=getResources().getConfiguration().screenWidthDp>=700&&index<2;if(compactTransport)card.removeView(timelines[index]);
        row(card,button("Load",color,()->choose(index)),button("Play / Pause",color,()->{if(engine.active(index))engine.pause(index);else{if(index<2)host.beforeMainPlay();engine.play(index);}}),button("Stop",color,()->{if(index<2&&engine.auto)engine.auto(false);engine.stop(index);}));
        View playControl=((LinearLayout)card.getChildAt(card.getChildCount()-1)).getChildAt(1);playControl.setTag("deck-play-"+index);playControl.setBackground(Chrome.button(colors,true));((Button)playControl).setTextColor(Color.BLACK);
        if(index<2){if(compactTransport){int position=card.getChildCount();card.addView(timelines[index]);UiSections.fold(card,colors,"Track position",position);}return;}int staging=card.getChildCount();if(compactTransport)card.addView(timelines[index]);row(card,button("−1s",color,()->engine.seek(index,engine.decks[index].getCurrentPosition()-1000)),button("+1s",color,()->engine.seek(index,engine.decks[index].getCurrentPosition()+1000)),button("Apply staged",color,()->engine.apply(index)));UiSections.fold(card,colors,"Seek & staged track",staging);
    }
    private void side(LinearLayout parent,int side){
        int color=side==0?DjEngine.AMBER:DjEngine.CYAN;LinearLayout card=card(parent);
        Button sideButton=button((engine.collapsed[side]?"▸ ":"▾ ")+(side==0?"Left":"Right")+" side deck",color,()->{engine.collapse(side);draw();});sideButton.setContentDescription(side==0?"Left side deck":"Right side deck");card.addView(sideButton);if(engine.collapsed[side])return;
        card.addView(label("Cue Deck · independent playback",17));transport(card,side+2,color);
        slider(card,100,(int)(engine.decks[side+2].getVolume()*100),p->engine.cueVolume(side,p/100f));
        row(card,button("Send → A",DjEngine.AMBER,()->send(side,0)),button("Send → B",DjEngine.CYAN,()->send(side,1)));
        card.addView(label("Uses this phone’s current audio output. No separate headphone monitoring on Android.",12));drop(card,side+2);
    }
    private void hotCueBank(LinearLayout parent){
        LinearLayout bank=card(parent); bank.setTag("dj-hot-cue-bank");
        bank.addView(label("Hot Cues · Deck "+(visibleDeck==0?"A":"B"),18));
        row(bank,button("Deck A cues",DjEngine.AMBER,()->{visibleDeck=0;engine.destination(0);draw();}),button("Deck B cues",DjEngine.CYAN,()->{visibleDeck=1;engine.destination(1);draw();}));
        bank.addView(label("Tap an empty cue to set it; tap a saved cue to jump. Hold to replace it.",13));
        final int deck=visibleDeck; MediaItem item=engine.decks[deck].getCurrentMediaItem();
        for(int r=0;r<2;r++){LinearLayout pads=new LinearLayout(activity);bank.addView(pads);for(int c=0;c<4;c++){
            final int slot=r*4+c;long position=item==null?-1:PhoneCues.seconds(activity,item,slot);
            Button pad=button("Cue "+(slot+1)+"\n"+(position<0?"Set":MainActivity.time(position)),deck==0?DjEngine.AMBER:DjEngine.CYAN,()->{
                MediaItem current=engine.decks[deck].getCurrentMediaItem();if(current==null){engine.message="Load a song on Deck "+(deck==0?"A":"B")+" before setting cues.";return;}
                long saved=PhoneCues.seconds(activity,current,slot);if(saved>=0)engine.seek(deck,saved);else PhoneCues.save(activity,current,slot,engine.decks[deck].getCurrentPosition());draw();
            });pad.setTag("dj-hot-cue-"+slot);pad.setContentDescription("Deck "+(deck==0?"A":"B")+" hot cue "+(slot+1));pad.setOnLongClickListener(v->{MediaItem current=engine.decks[deck].getCurrentMediaItem();if(current==null)return true;PhoneCues.save(activity,current,slot,engine.decks[deck].getCurrentPosition());draw();return true;});
            pads.addView(pad,new LayoutParams(0,dp(64),1));
        }}
    }
    private void samplerBank(LinearLayout parent){
        LinearLayout bank=card(parent);bank.setTag("dj-sampler-bank");final int side=visibleDeck;
        int color=side==0?DjEngine.AMBER:DjEngine.CYAN;bank.addView(label("Sampler · Bank "+(side==0?"A":"B"),18));
        row(bank,button("Bank A",DjEngine.AMBER,()->{visibleDeck=0;draw();}),button("Bank B",DjEngine.CYAN,()->{visibleDeck=1;draw();}));
        bank.addView(label("Effect pads · hold to configure",13));slider(bank,100,(int)(engine.samplerVolume[side]*100),p->engine.sampleVolume(side,p/100f));
        for(int r=0;r<3;r++){LinearLayout pads=new LinearLayout(activity);bank.addView(pads);for(int c=0;c<3;c++){
            final int slot=r*3+c;Button pad=decorate(new SquarePad(activity),engine.names[side][slot],color,()->{if(engine.samples[side][slot].isEmpty())host.effect(side,slot);else engine.trigger(side,slot);});
            pad.setTag("dj-sample-"+slot);pad.setContentDescription("Sampler "+(side==0?"A":"B")+" pad "+(slot+1));pad.setOnLongClickListener(v->{padMenu(side,slot);return true;});pads.addView(pad,new LayoutParams(0,dp(80),1));
        }}bank.addView(button("Stop all pads",color,()->{for(int s=0;s<2;s++)for(int slot=0;slot<9;slot++)engine.stopPad(s,slot);}));
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
        new DjSongPicker(activity,colors,stationAndPhoneSongs(),item->{engine.load(target,item,0);refresh();}).show(target);
    }
    private void renderSongs(LinearLayout rows,String query){rows.removeAllViews();int shown=0;for(MediaItem item:(engine.autoQueue.isEmpty()?stationAndPhoneSongs():new ArrayList<>(engine.autoQueue))){if(!String.valueOf(item.mediaMetadata.title).toLowerCase(Locale.ROOT).contains(query.toLowerCase(Locale.ROOT)))continue;if(shown++>=100)break;LinearLayout row=new LinearLayout(activity);Button title=button(String.valueOf(item.mediaMetadata.title),colors.text,()->{engine.load(engine.destination,item,0);refresh();});title.setGravity(Gravity.START|Gravity.CENTER_VERTICAL);title.setOnLongClickListener(v->{v.startDragAndDrop(ClipData.newPlainText("AMP song",item.mediaId),new View.DragShadowBuilder(v),item,0);return true;});row.addView(title,new LayoutParams(0,dp(52),1));for(int d=0;d<2;d++){final int deck=d;row.addView(button(d==0?"A":"B",d==0?DjEngine.AMBER:DjEngine.CYAN,()->{engine.destination(deck);engine.load(deck,item,0);}),new LayoutParams(dp(50),dp(52)));}row.addView(button("⋮",colors.text,()->new AlertDialog.Builder(activity).setTitle("Play within how many songs?").setItems(new String[]{"1","2","3","4","5"},(dialog,n)->{engine.request(item,n+1);refresh();}).show()),new LayoutParams(dp(44),dp(52)));rows.addView(row);}if(shown==0)rows.addView(label("No matching songs. Add phone music in Library.",14));if(shown>100)rows.addView(label("First 100 matches. Refine your search.",12));}
    private void drop(View view,int deck){view.setOnDragListener((v,event)->{if(!(event.getLocalState() instanceof MediaItem))return false;switch(event.getAction()){case DragEvent.ACTION_DRAG_ENTERED:v.setAlpha(.65f);break;case DragEvent.ACTION_DRAG_EXITED:case DragEvent.ACTION_DRAG_ENDED:v.setAlpha(1);break;case DragEvent.ACTION_DROP:engine.load(deck,(MediaItem)event.getLocalState(),0);v.setAlpha(1);refresh();break;default:break;}return true;});}
    private java.util.List<MediaItem> stationAndPhoneSongs(){java.util.LinkedHashMap<String,MediaItem> items=new java.util.LinkedHashMap<>();for(MediaItem item:LibraryStore.read(activity))items.put(item.mediaId,item);for(MediaItem item:RadioOffline.all(activity))items.putIfAbsent(item.mediaId,item);for(MediaItem item:engine.autoQueue)items.putIfAbsent(item.mediaId,item);return new java.util.ArrayList<>(items.values());}
    private void cues(int deck){MediaItem item=engine.decks[deck].getCurrentMediaItem();if(item==null)return;String[] options=new String[9];for(int n=0;n<8;n++){long value=PhoneCues.seconds(activity,item,n);options[n]="Cue "+(n+1)+" · "+(value>=0?MainActivity.time(value):"empty");}options[8]="Save current position to cue…";new AlertDialog.Builder(activity).setTitle("Hot cues · phone edits stay local").setItems(options,(d,n)->{if(n<8){long value=PhoneCues.seconds(activity,item,n);if(value>=0)engine.seek(deck,value);}else new AlertDialog.Builder(activity).setTitle("Save cue slot").setItems(new String[]{"1","2","3","4","5","6","7","8"},(a,slot)->PhoneCues.save(activity,item,slot,engine.decks[deck].getCurrentPosition())).show();}).setNegativeButton("Close",null).show();}
    private void sync(int deck){MediaItem own=engine.decks[deck].getCurrentMediaItem(),other=engine.decks[1-deck].getCurrentMediaItem();if(own==null||other==null){engine.message="Load both tracks first.";return;}double a=PhonePlaylists.bpm(activity,own),b=PhonePlaylists.bpm(activity,other);if(a<=0||b<=0){engine.message="Set BPM in Library track details before tempo sync.";return;}engine.decks[deck].setPlaybackSpeed((float)SharedDjLogic.tempoRate(b,engine.decks[1-deck].getPlaybackParameters().speed,a,.16));engine.message="Tempo matched within ±16%; beat phase is manual.";}
    void refresh(){
        engine.prepareScratch();
        for(int i=0;i<4;i++)if(info[i]!=null){ExoPlayer deck=engine.decks[i];long duration=deck.getDuration();MediaItem item=deck.getCurrentMediaItem();String id=item==null?"":item.mediaId;if(!Objects.equals(coverIds[i],id)||!Arrays.equals(coverData[i],item==null?null:item.mediaMetadata.artworkData)){coverIds[i]=id;coverData[i]=item==null?null:item.mediaMetadata.artworkData;Chrome.artwork(covers[i],item,i%2==0?DjEngine.AMBER:DjEngine.CYAN);}
            double bpm=item==null?0:PhonePlaylists.bpm(activity,item);String key=item==null?"":activity.getSharedPreferences("phone-track-details",0).getString(item.mediaId+":key","");
            info[i].setText(engine.title(i)+"\n"+MainActivity.time(engine.scratchPosition(i))+" / "+(duration>0?MainActivity.time(duration):"--:--")+" · "+(item==null?"Empty":engine.active(i)?"Playing":"Paused")+(bpm>0?"\n"+String.format(Locale.ROOT,"%.1f BPM",bpm):"")+(key.isEmpty()?"":" · "+key)+(engine.staged[i]!=null?" · replacement staged":""));
            if(timelines[i]!=null&&!timelines[i].isPressed())timelines[i].setProgress(duration>0?(int)(engine.scratchPosition(i)*1000/duration):0);
        }
        for(int i=0;i<2;i++)if(deckSelectors[i]!=null){deckSelectors[i].setText("DECK "+(i==0?"A":"B")+"\n"+engine.title(i));deckSelectors[i].setContentDescription("Select deck "+(i==0?"A":"B")+", "+engine.title(i));}
        if(secondaryInfo!=null){int other=1-visibleDeck;secondaryInfo.setText("DECK "+(other==0?"A":"B")+" · "+engine.title(other)+"\n"+(engine.staged[other]==null?"No replacement staged":"Staged: "+engine.staged[other].mediaMetadata.title));}
        if(status!=null)status.setText(engine.message);if(autoToggle!=null){updatingAuto=true;autoToggle.setChecked(engine.auto);updatingAuto=false;}if(fader!=null&&!fader.isPressed())fader.setProgress((int)(engine.fader*1000));if(mix!=null)mix.setText(engine.mixing()?"Cancel crossfade":"Mix next · "+engine.fadeSeconds+"s");
    }
    @Override protected void onAttachedToWindow(){super.onAttachedToWindow();timer.post(tick);}
    @Override protected void onDetachedFromWindow(){timer.removeCallbacks(tick);super.onDetachedFromWindow();}
    void redraw(){draw();}
    @android.annotation.SuppressLint("AppCompatCustomView") // Main player retains its native Activity/theme; this pad is not used in the AppCompat Cast picker.
    private static final class SquarePad extends Button {
        SquarePad(Context context){super(context);}
        @Override protected void onMeasure(int widthSpec,int heightSpec){super.onMeasure(widthSpec,heightSpec);int width=getMeasuredWidth();if(width>0)setMeasuredDimension(width,width);}
    }
    private final class Jog extends View {
        private long token;
        private final Paint paint=new Paint(3);private final int color,deck;private final VinylGesture gesture=new VinylGesture();private boolean held;private int pointer=-1;private float rotation;
        Jog(Context c,int color,int deck){super(c);this.color=color;this.deck=deck;setContentDescription("Vinyl scratch "+(deck<2?"deck "+(deck==0?"A":"B"):"cue deck "+(deck-1))+": hold and rotate forward or backward");}
        @Override protected void onDraw(Canvas canvas){super.onDraw(canvas);float x=getWidth()/2f,y=getHeight()/2f,r=Math.min(x,y)-6;paint.setStyle(Paint.Style.FILL);paint.setColor(Color.rgb(21,24,30));canvas.drawCircle(x,y,r,paint);paint.setStyle(Paint.Style.STROKE);paint.setStrokeWidth(3);paint.setColor(color);canvas.drawCircle(x,y,r,paint);paint.setStrokeWidth(1);paint.setColor(Color.argb(100,140,150,165));for(float ring=.43f;ring<.97f;ring+=.055f)canvas.drawCircle(x,y,r*ring,paint);paint.setStyle(Paint.Style.FILL);paint.setColor(color);canvas.drawCircle(x,y,r*.36f,paint);paint.setColor(Color.rgb(20,25,34));canvas.drawCircle(x,y,r*.29f,paint);paint.setStyle(Paint.Style.STROKE);paint.setColor(color);paint.setStrokeWidth(3);canvas.save();canvas.rotate(rotation,x,y);canvas.drawLine(x+r*.4f,y,x+r*.94f,y,paint);canvas.restore();paint.setStyle(Paint.Style.FILL);paint.setColor(color);paint.setTextAlign(Paint.Align.CENTER);paint.setTextSize(r*.38f);canvas.drawText("ϟ",x,y+r*.13f,paint);}

        @Override public boolean onTouchEvent(android.view.MotionEvent event){
            int action=event.getActionMasked();double radius=Math.max(1,Math.min(getWidth(),getHeight())/2.-6);
            if(action==android.view.MotionEvent.ACTION_DOWN){if(!VinylGesture.hit(event.getX()-getWidth()/2.,event.getY()-getHeight()/2.,radius,dp(8)))return false;held=engine.beginScratch(deck);if(!held){refresh();return false;}token=engine.scratchToken(deck);pointer=event.getPointerId(0);gesture.downPixels(event.getX()-getWidth()/2.,event.getY()-getHeight()/2.,radius);getParent().requestDisallowInterceptTouchEvent(true);setPressed(true);return true;}
            if(!held)return false;
            if(action==android.view.MotionEvent.ACTION_MOVE){int index=event.findPointerIndex(pointer);if(index<0){release(true);return true;}double seconds=gesture.movePixels(event.getX(index)-getWidth()/2.,event.getY(index)-getHeight()/2.,radius);rotation+=(float)(seconds/1.8*360);engine.dragScratch(deck,token,seconds);invalidate();return true;}
            if(action==android.view.MotionEvent.ACTION_UP){release(true);performClick();return true;}
            if(action==android.view.MotionEvent.ACTION_CANCEL||(action==android.view.MotionEvent.ACTION_POINTER_UP&&event.getPointerId(event.getActionIndex())==pointer)){release(true);return true;}
            return true;
        }
        private void release(boolean resume){if(token!=0)engine.endScratch(deck,token,resume);if(held){held=false;gesture.up();setPressed(false);if(getParent()!=null)getParent().requestDisallowInterceptTouchEvent(false);}}
        @Override public void onWindowFocusChanged(boolean focused){super.onWindowFocusChanged(focused);if(!focused)release(false);}
        @Override protected void onDetachedFromWindow(){release(true);super.onDetachedFromWindow();}
        @Override public boolean performClick(){super.performClick();return true;}
    }
}
