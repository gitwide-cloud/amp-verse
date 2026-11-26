package cloud.gitwide.ampverse;
import android.app.*;
import android.widget.*;
import org.json.*;

/** Touch-sized two-deck surface; all commands go to the core's DJ engine. */
final class FeatureScreen {
    final CompanionActivity a;
    final TextView[] titles=new TextView[4];
    final SeekBar[] positions=new SeekBar[4];
    Switch auto;
    SeekBar fader;
    boolean updating,closed,polling;
    int destination;
    final Runnable tick=new Runnable(){public void run(){if(closed)return;if(a.active&&!polling){polling=true;a.rpc("state",null,FeatureScreen.this::render,()->polling=false);}a.handler.postDelayed(this,500);}};
    FeatureScreen(CompanionActivity a){this.a=a;}
    JSONObject args(Object... pairs){JSONObject j=new JSONObject();try{for(int i=0;i<pairs.length;i+=2)j.put(String.valueOf(pairs[i]),pairs[i+1]);}catch(Exception ignored){}return j;}
    void send(String action,Object... pairs){a.rpc(action,args(pairs),this::render);}
    void open(LinearLayout parent){
        a.row(parent,a.button("Choose playlist",this::playlists),a.button("Load song",()->browse("",0)));
        auto=new Switch(a);auto.setText("Auto Mix · off = manual");auto.setTextColor(a.colors.text);auto.setMinimumHeight(a.dp(48));Chrome.toggle(auto,a.colors);auto.setOnCheckedChangeListener((v,on)->{if(!updating)send("auto","enabled",on);});parent.addView(auto);
        a.row(parent,a.button("Mix next",()->send("mix")),a.button("Fade −1s",()->a.rpc("state",null,s->send("fade","seconds",s.optInt("fade",3)-1))),a.button("Fade +1s",()->a.rpc("state",null,s->send("fade","seconds",s.optInt("fade",3)+1))));
        for(int deck=0;deck<4;deck++){
            final int d=deck;LinearLayout card=a.column();card.setBackground(Chrome.panel(a.colors,false));card.setPadding(a.dp(6),a.dp(8),a.dp(6),a.dp(8));parent.addView(card);
            titles[d]=a.label((d==0?"DECK A":d==1?"DECK B":d==2?"CUE A":"CUE B")+" · no track",18);titles[d].setTextColor(d%2==0?0xffffbf24:0xff22d4ed);card.addView(titles[d]);
            a.row(card,a.button("Load here",()->{destination=d;browse("",0);}),a.button("▶",()->send("play","deck",d)),a.button("Pause",()->send("pause","deck",d)),a.button("Cue",()->send("cue","deck",d)));
            positions[d]=new SeekBar(a);positions[d].setMinimumHeight(a.dp(48));positions[d].setMax(1000);positions[d].setContentDescription("Seek "+(d==0?"deck A":d==1?"deck B":"cue deck"));Chrome.slider(positions[d],d%2==0?0xffffbf24:0xff22d4ed);positions[d].setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){}public void onProgressChanged(SeekBar b,int value,boolean user){}public void onStopTrackingTouch(SeekBar b){Object duration=b.getTag();if(duration instanceof Long)send("seek","deck",d,"position",((Long)duration)*b.getProgress()/1000);}});card.addView(positions[d]);
            if(d<2){Button[] cues=new Button[4];for(int n=0;n<4;n++){final int slot=n;cues[n]=a.button("Cue "+(n+1),()->send("hotcue","deck",d,"slot",slot));cues[n].setOnLongClickListener(v->{send("hotcue","deck",d,"slot",slot,"save",true);return true;});}a.row(card,cues);}
        }
        parent.addView(a.label("Crossfader · A ↔ B",16));fader=new SeekBar(a);fader.setMax(1000);fader.setMinimumHeight(a.dp(48));fader.setContentDescription("DJ crossfader");Chrome.slider(fader,a.colors.accent);fader.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){}public void onProgressChanged(SeekBar b,int value,boolean user){if(user)send("fader","value",value/1000.);}public void onStopTrackingTouch(SeekBar b){}});parent.addView(fader);
        parent.addView(a.label("Sampler · saved pads",18));for(int side=0;side<2;side++){final int s=side;for(int row=0;row<3;row++){Button[] pads=new Button[3];for(int col=0;col<3;col++){final int pad=row*3+col;pads[col]=a.button((s==0?"A":"B")+" · "+(pad+1),()->send("sample","side",s,"pad",pad));}a.row(parent,pads);}}
        tick.run();
    }
    void render(JSONObject state)throws Exception{updating=true;auto.setChecked(state.optBoolean("auto"));if(!fader.isPressed())fader.setProgress((int)(state.optDouble("fader")*1000));JSONArray decks=state.getJSONArray("decks");for(int i=0;i<4;i++){JSONObject deck=decks.getJSONObject(i);titles[i].setText((i==0?"A":i==1?"B":i==2?"Cue A":"Cue B")+" · "+deck.optString("title"));long duration=deck.optLong("duration");positions[i].setTag(duration);if(!positions[i].isPressed())positions[i].setProgress(duration>0?(int)(1000*deck.optLong("position")/duration):0);}a.status.setText(state.optString("message")+" · Fade "+state.optInt("fade",3)+"s");updating=false;}
    void playlists(){a.rpc("playlists",null,value->{JSONArray lists=value.getJSONArray("items");String[] names=new String[lists.length()];for(int i=0;i<names.length;i++)names[i]=lists.getJSONObject(i).optString("name");new AlertDialog.Builder(a).setTitle("Phone playlists").setItems(names,(d,i)->send("queue","id",lists.optJSONObject(i).optString("id"))).setNegativeButton("Cancel",null).show();});}
    void browse(String query,int offset){a.rpc("library",args("query",query,"offset",offset),value->{JSONArray items=value.getJSONArray("items");LinearLayout form=a.column();EditText search=new EditText(a);search.setText(query);search.setHint("Song or artist");search.setTextColor(a.colors.text);form.addView(search);form.addView(a.button("Search",()->browse(search.getText().toString(),0)));String[] names=new String[items.length()];for(int i=0;i<names.length;i++)names[i]=items.getJSONObject(i).optString("title");ListView list=new ListView(a);list.setAdapter(new ArrayAdapter<>(a,android.R.layout.simple_list_item_1,names));form.addView(list,new LinearLayout.LayoutParams(-1,a.dp(280)));AlertDialog dialog=new AlertDialog.Builder(a).setTitle("Load into "+(destination==0?"A":destination==1?"B":destination==2?"Cue A":"Cue B")).setView(form).setNegativeButton("Close",null).create();list.setOnItemClickListener((p,v,i,id)->{send("load","deck",destination,"id",items.optJSONObject(i).optString("id"));dialog.dismiss();});if(offset+items.length()<value.optInt("total"))form.addView(a.button("Next 50 songs",()->{dialog.dismiss();browse(query,offset+50);}));dialog.show();});}
    void close(){closed=true;a.handler.removeCallbacks(tick);}
}
