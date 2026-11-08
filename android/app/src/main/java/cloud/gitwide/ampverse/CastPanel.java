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
    private final EditText address,password;
    private final Button connect,start,stop,listen,refresh;
    private CastClient client;
    private boolean closed,busy;
    private String stationName="AMP Cast";
    CastPanel(Activity activity,Appearance colors,Listener listener) {
        super(activity);this.activity=activity;this.colors=colors;this.listener=listener;
        setOrientation(VERTICAL);setPadding(dp(10),dp(10),dp(10),dp(10));setBackground(Chrome.panel(colors,false));
        addView(label("AMP Cast · Radio",20));
        addView(label("Connect to your running AMP Cast server. The phone controls or listens to that station; it does not host the server.",14));
        address=field("http://192.168.1.50:8787",false);address.setContentDescription("AMP Cast server address");address.setText(activity.getSharedPreferences("settings",Context.MODE_PRIVATE).getString("cast-server",""));
        password=field("Optional admin password",true);password.setContentDescription("AMP Cast admin password");
        connect=action("Connect",()->connect());addView(connect);
        status=label("Not connected",14);addView(status);
        now=label("No station selected",18);addView(now);
        LinearLayout controls=new LinearLayout(activity);start=action("Start station",()->command(true));stop=action("Stop station",()->command(false));controls.addView(start,new LinearLayout.LayoutParams(0,dp(50),1));controls.addView(stop,new LinearLayout.LayoutParams(0,dp(50),1));addView(controls);
        listen=action("Listen on this phone",()->{if(client!=null)listener.listen(client.stream(),stationName);});addView(listen);
        addView(action("Stop phone playback",listener::stopListening));
        refresh=action("Refresh station",()->refresh());addView(refresh);
        addView(label("Station queue",17));queue=label("Connect to see the server's playlist.",14);addView(queue);
        addView(label("Station playlists are prepared on desktop. Offline downloads, Google Cast output and cross-device synchronization are not available here yet.",12));enabled(false);
    }
    private int dp(int n){return (int)(n*getResources().getDisplayMetrics().density);}
    private TextView label(String text,int size){TextView v=new TextView(activity);v.setText(text);v.setTextColor(colors.text);v.setTextSize(size);v.setPadding(0,dp(6),0,dp(6));return v;}
    private EditText field(String hint,boolean secret){EditText v=new EditText(activity);v.setHint(hint);v.setSingleLine(true);v.setTextColor(colors.text);v.setHintTextColor(colors.muted);v.setInputType(InputType.TYPE_CLASS_TEXT|(secret?InputType.TYPE_TEXT_VARIATION_PASSWORD:InputType.TYPE_TEXT_VARIATION_URI));addView(v);return v;}
    private Button action(String text,Runnable action){Button b=new Button(activity);b.setText(text);b.setAllCaps(false);b.setTextColor(colors.classic?android.graphics.Color.BLACK:colors.text);b.setBackground(Chrome.button(colors,false));b.setMinHeight(dp(48));b.setOnClickListener(v->action.run());return b;}
    private void enabled(boolean connected){start.setEnabled(connected&&!busy);stop.setEnabled(connected&&!busy);listen.setEnabled(connected&&!busy);refresh.setEnabled(connected&&!busy);connect.setEnabled(!busy);address.setEnabled(!busy);password.setEnabled(!busy);}
    private interface Operation {String run()throws Exception;}
    private void work(Operation operation){
        if(busy||closed)return;busy=true;enabled(client!=null);status.setText("Connecting / updating station…");
        worker.execute(()->{String message;try{message=operation.run();}catch(Exception error){message="Could not update station: "+(error.getMessage()==null?error.getClass().getSimpleName():error.getMessage());}
            final String result=message;activity.runOnUiThread(()->{if(closed)return;busy=false;status.setText(result);enabled(client!=null);});});
    }
    private void connect(){
        final String url=address.getText().toString(),secret=password.getText().toString();client=null;enabled(false);
        work(()->{CastClient candidate=new CastClient(url,secret);JSONObject snapshot=candidate.connect();JSONObject list=candidate.playlist();
            activity.runOnUiThread(()->{if(closed)return;client=candidate;activity.getSharedPreferences("settings",Context.MODE_PRIVATE).edit().putString("cast-server",candidate.base).apply();render(snapshot,list);});return "Connected to selected AMP Cast server";});
    }
    private void refresh(){final CastClient selected=client;if(selected==null)return;work(()->{JSONObject snapshot=selected.status(),list=selected.playlist();activity.runOnUiThread(()->{if(!closed&&client==selected)render(snapshot,list);});return "Station refreshed";});}
    private void command(boolean launch){final CastClient selected=client;if(selected==null)return;work(()->{selected.command(launch);JSONObject snapshot=selected.status(),list=selected.playlist();activity.runOnUiThread(()->{if(!closed&&client==selected)render(snapshot,list);});return launch?"Station started":"Station stopped";});}
    private void render(JSONObject snapshot,JSONObject list){
        stationName=snapshot.optString("station_name","AMP Cast");
        now.setText(stationName+" · "+(snapshot.optBoolean("running")?"ON AIR":"Stopped")+"\n"+(snapshot.isNull("now_playing")?"No track playing":snapshot.optString("now_playing",""))+"\nListeners: "+snapshot.optInt("listeners")+(snapshot.isNull("error")?"":"\n"+snapshot.optString("error","")));
        JSONArray tracks=list.optJSONArray("tracks");StringBuilder text=new StringBuilder();
        if(tracks!=null)for(int i=0;i<Math.min(tracks.length(),200);i++){JSONObject t=tracks.optJSONObject(i);if(t!=null)text.append(i+1).append(". ").append(t.optString("title","Track")).append('\n');}
        if(tracks!=null&&tracks.length()>200)text.append("Showing the first 200 songs.");
        queue.setText(text.length()==0?"The server queue is empty. Add music on desktop.":text.toString());
    }
    void close(){closed=true;worker.shutdownNow();}
}
