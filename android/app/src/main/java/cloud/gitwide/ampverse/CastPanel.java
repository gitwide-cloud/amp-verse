package cloud.gitwide.ampverse;

import android.app.Activity;
import android.content.Context;
import android.text.InputType;
import android.widget.*;
import org.json.*;
import java.util.concurrent.*;

/** Integrated Android station controls; local listening uses the existing playback service. */
final class CastPanel extends LinearLayout {
    interface Listener {void listen(String url,String name);void stopListening();}
    private final Activity activity;
    private final Appearance colors;
    private final Listener listener;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final TextView status,now,queue;
    private final EditText address,password,search;
    private final LinearLayout serverList,connectionFields;
    private org.json.JSONArray tracks=new org.json.JSONArray();
    private Button find;
    private final Button connect,start,stop,listen,refresh;
    private CastClient client;
    private boolean closed,busy,running;
    private String stationName="AMP Cast";
    CastPanel(Activity activity,Appearance colors,Listener listener) {
        super(activity);this.activity=activity;this.colors=colors;this.listener=listener;
        setOrientation(VERTICAL);setPadding(dp(10),dp(10),dp(10),dp(10));setBackground(Chrome.panel(colors,false));
        addView(label("Your radio station",20));
        find=action("Find servers",this::findServers);addView(find);
        serverList=new LinearLayout(activity);serverList.setOrientation(VERTICAL);addView(serverList);
        connectionFields=new LinearLayout(activity);connectionFields.setOrientation(VERTICAL);
        Button manual=action("Server settings / enter address",()->connectionFields.setVisibility(connectionFields.getVisibility()==VISIBLE?GONE:VISIBLE));addView(manual);
        addView(connectionFields);connectionFields.setVisibility(GONE);
        address=field("http://192.168.1.50:8787",false);address.setContentDescription("AMP Cast server address");address.setText(activity.getSharedPreferences("settings",Context.MODE_PRIVATE).getString("cast-server",""));
        password=field("Optional admin password",true);password.setContentDescription("AMP Cast admin password");
        connect=action("Connect",()->connect());connectionFields.addView(connect);
        status=label("Not connected",14);addView(status);
        now=label("No station selected",18);addView(now);
        LinearLayout controls=new LinearLayout(activity);start=action("Start station",()->command(true));stop=action("Stop station",()->command(false));controls.addView(start,new LinearLayout.LayoutParams(0,dp(50),1));controls.addView(stop,new LinearLayout.LayoutParams(0,dp(50),1));addView(controls);
        listen=action("Listen on this phone",()->{if(client!=null)listener.listen(client.stream(),stationName);});addView(listen);
        addView(action("Stop phone playback",listener::stopListening));
        refresh=action("Refresh station",()->refresh());addView(refresh);
        addView(label("Music",17));
        search=new EditText(activity);search.setSingleLine(true);search.setTextColor(colors.text);search.setHintTextColor(colors.muted);search.setHint("Search songs in this station");search.setContentDescription("Search station songs");addView(search);
        queue=label("Find a server to see its music.",14);addView(queue);
        search.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int a,int c,int d){}public void onTextChanged(CharSequence t,int a,int b,int c){renderQueue();}public void afterTextChanged(android.text.Editable t){}});
        addView(action("Help",()->new android.app.AlertDialog.Builder(activity).setTitle("Radio help").setMessage("Keep your phone and Cast server on the same Wi-Fi or LAN. Find servers needs an updated server. You can also enter its address in Server settings. Start/Stop changes the station; Stop phone playback only stops your phone. Prepare playlists in desktop Library.").setPositiveButton("Done",null).show()));enabled(false);
    }
    private int dp(int n){return (int)(n*getResources().getDisplayMetrics().density);}
    private TextView label(String text,int size){TextView v=new TextView(activity);v.setText(text);v.setTextColor(colors.text);v.setTextSize(size);v.setPadding(0,dp(6),0,dp(6));return v;}
    private EditText field(String hint,boolean secret){EditText v=new EditText(activity);v.setHint(hint);v.setSingleLine(true);v.setTextColor(colors.text);v.setHintTextColor(colors.muted);v.setInputType(InputType.TYPE_CLASS_TEXT|(secret?InputType.TYPE_TEXT_VARIATION_PASSWORD:InputType.TYPE_TEXT_VARIATION_URI));connectionFields.addView(v);return v;}
    private Button action(String text,Runnable action){Button b=new Button(activity);b.setText(text);b.setAllCaps(false);b.setTextColor(colors.classic?android.graphics.Color.BLACK:colors.text);b.setBackground(Chrome.button(colors,false));b.setMinHeight(dp(48));b.setOnClickListener(v->action.run());return b;}
    private void enabled(boolean connected){start.setEnabled(connected&&!busy&&!running);stop.setEnabled(connected&&!busy&&running);listen.setEnabled(connected&&!busy&&running);refresh.setEnabled(connected&&!busy);connect.setEnabled(!busy);find.setEnabled(!busy);address.setEnabled(!busy);password.setEnabled(!busy);for(Button button:new Button[]{start,stop,listen,refresh})button.setAlpha(button.isEnabled()?1f:.35f);}
    private interface Operation {String run()throws Exception;}
    private void work(Operation operation){
        if(busy||closed)return;busy=true;enabled(client!=null);status.setText("Connecting / updating station…");
        worker.execute(()->{String message;try{message=operation.run();}catch(Exception error){message="Could not update station: "+(error.getMessage()==null?error.getClass().getSimpleName():error.getMessage());}
            final String result=message;activity.runOnUiThread(()->{if(closed)return;busy=false;status.setText(result);enabled(client!=null);});});
    }
    private void connect(){
        final String url=address.getText().toString(),secret=password.getText().toString();client=null;enabled(false);
        work(()->{CastClient candidate=new CastClient(url,secret);JSONObject snapshot=candidate.connect();JSONObject list=candidate.playlist();
            activity.runOnUiThread(()->{if(closed)return;client=candidate;connectionFields.setVisibility(GONE);activity.getSharedPreferences("settings",Context.MODE_PRIVATE).edit().putString("cast-server",candidate.base).apply();render(snapshot,list);});return "Connected to selected AMP Cast server";});
    }
    private void refresh(){final CastClient selected=client;if(selected==null)return;work(()->{JSONObject snapshot=selected.status(),list=selected.playlist();activity.runOnUiThread(()->{if(!closed&&client==selected)render(snapshot,list);});return "Station refreshed";});}
    private void command(boolean launch){final CastClient selected=client;if(selected==null)return;work(()->{selected.command(launch);JSONObject snapshot=selected.status(),list=selected.playlist();activity.runOnUiThread(()->{if(!closed&&client==selected)render(snapshot,list);});return launch?"Station started":"Station stopped";});}
    private void render(JSONObject snapshot,JSONObject list){
        running=snapshot.optBoolean("running");
        stationName=snapshot.optString("station_name","AMP Cast");
        now.setText(stationName+" · "+(snapshot.optBoolean("running")?"ON AIR":"Stopped")+"\n"+(snapshot.isNull("now_playing")?"No track playing":snapshot.optString("now_playing",""))+"\nListeners: "+snapshot.optInt("listeners")+(snapshot.isNull("error")?"":"\n"+snapshot.optString("error","")));
        tracks=list.optJSONArray("tracks");if(tracks==null)tracks=new JSONArray();renderQueue();
    }
    private void renderQueue(){
        String query=search.getText().toString().trim().toLowerCase(java.util.Locale.ROOT);StringBuilder text=new StringBuilder();int total=0,shown=0;
        for(int i=0;i<tracks.length();i++){JSONObject track=tracks.optJSONObject(i);if(track==null)continue;String title=track.optString("title","Track");if(!title.toLowerCase(java.util.Locale.ROOT).contains(query))continue;total++;if(shown++<200)text.append(i+1).append(". ").append(title).append('\n');}
        if(total>200)text.append("Showing 200 of ").append(total).append(" matches. Refine your search.");
        queue.setText(text.length()==0?(client==null?"Find a server to see its music.":query.isEmpty()?"No songs in this station yet.":"No songs match your search."):text.toString());
    }
    private void findServers(){
        work(()->{java.util.List<CastDiscovery.Server> servers=CastDiscovery.find();activity.runOnUiThread(()->{if(closed)return;serverList.removeAllViews();for(CastDiscovery.Server server:servers){serverList.addView(action(server.name+" · "+server.url.replace("http://",""),()->{address.setText(server.url);password.setText("");connect();}));}});return servers.isEmpty()?"No servers found. Check Wi-Fi or enter the server address.":"Choose a server below.";});
    }
    void close(){closed=true;worker.shutdownNow();}
}
