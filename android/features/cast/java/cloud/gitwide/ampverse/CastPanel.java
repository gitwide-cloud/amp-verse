package cloud.gitwide.ampverse;

import android.app.Activity;
import android.content.Context;
import android.text.InputType;
import android.widget.*;
import org.json.*;
import java.util.concurrent.*;

/** Integrated Android station controls; local listening uses the existing playback service. */
final class CastPanel extends LinearLayout {
    interface Listener {void listen(String url,String name);void stopListening();default void playlist(java.util.List<androidx.media3.common.MediaItem> items){}}
    private final Activity activity;
    private final Appearance colors;
    private final Listener listener;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final TextView status,now,queue;
    private TextView capabilityNote;
    private Button chromecast,hostBroadcast,serverPlay,serverStop,stationPlaylist;
    private volatile boolean verified;
    private boolean hostLaunchSupported,serverOutputSupported,playlistSupported,serverOutputRunning;
    private final EditText address,password,search;
    private final LinearLayout serverList,connectionFields,sharedLists;
    private LinearLayout stationControls;
    private org.json.JSONArray tracks=new org.json.JSONArray();
    private Button find;
    private final Button connect,start,stop,listen,refresh;
    private CastClient client;
    private boolean closed,busy,running,onAir,preparing;
    private String stationName="AMP Cast";
    private final android.os.Handler handler=new android.os.Handler(android.os.Looper.getMainLooper());
    private final Runnable poll=new Runnable(){public void run(){if(closed)return;if(client!=null&&!busy&&verified)refresh(true);handler.postDelayed(this,3000);}};
    CastPanel(Activity activity,Appearance colors,Listener listener) {
        super(activity);this.activity=activity;this.colors=colors;this.listener=listener;
        setOrientation(VERTICAL);setPadding(dp(10),dp(10),dp(10),dp(10));setBackground(Chrome.panel(colors,false));
        addView(label("Cast · choose an AMP host",20));
        addView(label("Discovery and manual connection only select a host. They do not start or stop playback.",14));
        find=action("Find nearby stations",this::findServers);addView(find);
        serverList=new LinearLayout(activity);serverList.setOrientation(VERTICAL);addView(serverList);
        int broadcastStart=getChildCount();
        EditText radioQuery=new EditText(activity);radioQuery.setSingleLine(true);radioQuery.setHint("Search broadcast stations by name");radioQuery.setTextColor(colors.text);radioQuery.setHintTextColor(colors.muted);addView(radioQuery);
        addView(action("Search broadcast stations",()->{final String query=radioQuery.getText().toString().trim();if(query.isEmpty())return;work(()->{JSONArray stations=RadioDirectory.search(query);activity.runOnUiThread(()->{if(closed)return;serverList.removeAllViews();for(int i=0;i<stations.length();i++){JSONObject station=stations.optJSONObject(i);if(station==null)continue;String stream=station.optString("url_resolved");if(!stream.startsWith("http://")&&!stream.startsWith("https://"))continue;serverList.addView(action(station.optString("name")+" · Listen",()->listener.listen(stream,station.optString("name"))));}});return "Broadcast station results";},false,false);}));
        UiSections.fold(this,colors,"Internet radio",broadcastStart);
        connectionFields=new LinearLayout(activity);connectionFields.setOrientation(VERTICAL);
        Button manual=action("Server settings / enter address",()->connectionFields.setVisibility(connectionFields.getVisibility()==VISIBLE?GONE:VISIBLE));addView(manual);
        addView(connectionFields);connectionFields.setVisibility(GONE);
        address=field("http://192.168.1.50:8787",false);address.setContentDescription("AMP Cast server address");address.setText(activity.getSharedPreferences("settings",Context.MODE_PRIVATE).getString("cast-server",""));
        password=field("Optional admin password",true);password.setContentDescription("AMP Cast admin password");
        connect=action("Connect",()->connect());connectionFields.addView(connect);
        status=label("Not connected. A host bound to 127.0.0.1 is reachable only on that host. Use a LAN-enabled host.",14);addView(status);
        now=label("No station selected",18);addView(now);
        stationControls=new LinearLayout(activity);stationControls.setOrientation(VERTICAL);addView(stationControls);
        stationControls.addView(label("Destination · hear the host’s current stream",18));
        stationControls.addView(label("These destinations receive the selected host’s program, not this phone’s music. Buffered output may be delayed.",14));
        listen=action("Listen to host on this phone",()->{if(client!=null)listener.listen(client.phoneStream(),stationName);});stationControls.addView(listen);
        stationControls.addView(action("Stop phone playback",listener::stopListening));
        chromecast=action("Send host stream to Chromecast",()->{if(client==null||!onAir){status.setText("Connect and start a station or live DJ broadcast first.");return;}activity.startActivity(new android.content.Intent(activity,GoogleCastActivity.class).putExtra("stream",client.stream()).putExtra("title",stationName));});stationControls.addView(chromecast);
        serverPlay=action("Request playback on server speakers",()->{final CastClient selected=client;if(selected!=null&&verified&&serverOutputSupported)work(()->{selected.post("/api/output/start",null);sync(selected);return "Server output requested. Audio device availability and audibility are not confirmed.";});});stationControls.addView(serverPlay);
        serverStop=action("Stop server speaker only",()->{final CastClient selected=client;if(selected!=null&&verified&&serverOutputSupported)work(()->{selected.post("/api/output/stop",null);sync(selected);return "Server speaker stop requested; station continues.";});});stationControls.addView(serverStop);
        stationControls.addView(label("Source · change the selected host’s program",18));
        stationControls.addView(label("Source controls affect everyone listening to this host. Choosing a destination above does not change its source.",14));
        LinearLayout controls=new LinearLayout(activity);controls.setOrientation(VERTICAL);start=action("Start host station",()->command(true));stop=action("Stop host program for all listeners",()->command(false));controls.addView(start,new LinearLayout.LayoutParams(-1,-2));controls.addView(stop,new LinearLayout.LayoutParams(-1,-2));stationControls.addView(controls);
        refresh=action("Refresh host status",()->refresh());stationControls.addView(refresh);
        stationPlaylist=action("Choose independent Smart Radio playlist…",this::chooseStationPlaylist);stationControls.addView(stationPlaylist);
        hostBroadcast=action("Request desktop host Player/DJ master…",()->{final CastClient selected=client;if(selected==null||!verified||!hostLaunchSupported)return;EditText secret=new EditText(activity);secret.setHint("Desktop Player/DJ control password, if configured");secret.setInputType(129);new android.app.AlertDialog.Builder(activity).setTitle("Change host source to desktop master?").setMessage("This requests the Player/DJ on the host machine. It does not send phone audio. The desktop must be running with its control endpoint enabled; command support does not confirm it is reachable. Existing station listeners will hear the new program.").setView(secret).setPositiveButton("Request host master",(d,n)->{if(client!=selected||!verified||busy)return;final String value=secret.getText().toString();work(()->{selected.post("/api/live/launch",new JSONObject().put("source_password",value));sync(selected);return "Host sender requested; waiting for master audio. Phone audio was not sent.";});}).setNegativeButton("Cancel",null).show();});stationControls.addView(hostBroadcast);
        Button phoneSend=action("Send this phone’s current playback",()->{});phoneSend.setEnabled(false);phoneSend.setAlpha(.35f);stationControls.addView(phoneSend);
        stationControls.addView(label("Phone-source sending is not implemented in this Android build. Station listening and Chromecast forwarding do not capture the phone Player or DJ master.",14));
        capabilityNote=label("Host capabilities have not been checked.",14);stationControls.addView(capabilityNote);
        sharedLists=new LinearLayout(activity);sharedLists.setOrientation(VERTICAL);addView(sharedLists);
        addView(action("Offline playlists",this::offline));

        search=new EditText(activity);search.setSingleLine(true);search.setTextColor(colors.text);search.setHintTextColor(colors.muted);search.setHint("Search songs in this station");search.setContentDescription("Search station songs");addView(search);
        queue=label("Find a server to see its music.",14);addView(queue);
        search.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int a,int c,int d){}public void onTextChanged(CharSequence t,int a,int b,int c){renderQueue();}public void afterTextChanged(android.text.Editable t){}});
        addView(action("Help",()->new android.app.AlertDialog.Builder(activity).setTitle("Radio help").setMessage("Search LAN stations to find a nearby AMP host, or search broadcast stations by name. For your own music, choose Host Smart Station on the desktop, connect here, and Save offline with my cues. Offline playlists remain separate from phone-folder music.").setPositiveButton("Done",null).show()));enabled(false);
        handler.postDelayed(poll,3000);
    }
    private int dp(int n){return (int)(n*getResources().getDisplayMetrics().density);}
    private TextView label(String text,int size){TextView v=new TextView(activity);v.setText(text);v.setTextColor(colors.text);v.setTextSize(size);v.setPadding(0,dp(6),0,dp(6));return v;}
    private EditText field(String hint,boolean secret){EditText v=new EditText(activity);v.setHint(hint);v.setSingleLine(true);v.setTextColor(colors.text);v.setHintTextColor(colors.muted);v.setInputType(InputType.TYPE_CLASS_TEXT|(secret?InputType.TYPE_TEXT_VARIATION_PASSWORD:InputType.TYPE_TEXT_VARIATION_URI));connectionFields.addView(v);return v;}
    private Button action(String text,Runnable action){Button b=new Button(activity);b.setText(text);b.setAllCaps(false);b.setTextColor(Chrome.ink(colors,false));b.setBackground(Chrome.button(colors,false));b.setMinHeight(dp(48));b.setOnClickListener(v->action.run());return b;}
    private void enabled(boolean connected){
        stationControls.setVisibility(connected?VISIBLE:GONE);search.setVisibility(connected?VISIBLE:GONE);queue.setVisibility(connected?VISIBLE:GONE);
        boolean ready=connected&&verified&&!busy;
        start.setEnabled(ready&&playlistSupported&&!running&&!preparing);stop.setEnabled(ready&&(running||preparing));listen.setEnabled(ready&&onAir);chromecast.setEnabled(ready&&onAir);
        hostBroadcast.setEnabled(ready&&hostLaunchSupported);serverPlay.setEnabled(ready&&serverOutputSupported&&onAir&&!serverOutputRunning);serverStop.setEnabled(ready&&serverOutputSupported&&serverOutputRunning);stationPlaylist.setEnabled(ready&&playlistSupported);
        setChildrenEnabled(sharedLists,ready);
        refresh.setEnabled(connected&&!busy);connect.setEnabled(!busy);find.setEnabled(!busy);address.setEnabled(!busy);password.setEnabled(!busy);
        for(Button button:new Button[]{start,stop,listen,refresh,chromecast,hostBroadcast,serverPlay,serverStop,stationPlaylist})button.setAlpha(button.isEnabled()?1f:.35f);
    }
    private void setChildrenEnabled(android.view.View view,boolean value){
        view.setEnabled(value);if(view instanceof Button)view.setAlpha(value?1f:.35f);
        if(view instanceof android.view.ViewGroup){android.view.ViewGroup group=(android.view.ViewGroup)view;for(int i=0;i<group.getChildCount();i++)setChildrenEnabled(group.getChildAt(i),value);}
    }
    private interface Operation {String run()throws Exception;}
    private void work(Operation operation){work(operation,false,true);}
    private void work(Operation operation,boolean background,boolean affectsHost){
        if(busy||closed)return;busy=true;enabled(client!=null);if(!background)status.setText("Connecting / updating host…");
        worker.execute(()->{String message;boolean failed=false;try{message=operation.run();}catch(Exception error){failed=true;if(affectsHost)verified=false;message="Could not update: "+(error.getMessage()==null?error.getClass().getSimpleName():error.getMessage());}
            final String result=message;final boolean error=failed;activity.runOnUiThread(()->{if(closed)return;busy=false;if(!background||error)status.setText(result);enabled(client!=null);});});
    }
    private boolean selected(CastClient candidate){return candidate!=null&&client==candidate&&verified&&!busy&&!closed;}
    private void sync(CastClient selected)throws Exception{
        JSONObject snapshot=selected.status(),list=selected.playlist();
        activity.runOnUiThread(()->{if(!closed&&client==selected)render(snapshot,list);});
    }
    private void connect(){connect(null);}
    private void connect(String expectedIdentity){
        if(busy||closed)return;
        final String url=address.getText().toString(),secret=password.getText().toString();client=null;verified=false;running=false;hostLaunchSupported=false;serverOutputSupported=false;playlistSupported=false;serverOutputRunning=false;onAir=false;preparing=false;now.setText("No host selected");capabilityNote.setText("Host capabilities have not been checked.");tracks=new JSONArray();sharedLists.removeAllViews();enabled(false);
        work(()->{CastClient.requireLanAddress(url);CastClient candidate=new CastClient(url,secret);JSONObject snapshot=candidate.connect();if(expectedIdentity!=null&&!expectedIdentity.equals(snapshot.optString("server_id")+"/"+snapshot.optString("station_id")))throw new java.io.IOException("Discovered host identity changed. Select the host again.");candidate.get("/api/station/playlists");JSONObject list=candidate.playlist();JSONObject manifest=snapshot.optJSONObject("capabilities")!=null&&snapshot.getJSONObject("capabilities").optBoolean("smart_sharing")?candidate.get("/api/smart/manifest"):null;
            activity.runOnUiThread(()->{if(closed)return;client=candidate;verified=true;connectionFields.setVisibility(GONE);activity.getSharedPreferences("settings",Context.MODE_PRIVATE).edit().putString("cast-server",candidate.base).apply();render(snapshot,list);renderShared(manifest);});return "Host connected. Choose a destination; no playback was started";});
    }
    private void refresh(){refresh(false);}
    private void refresh(boolean background){final CastClient selected=client;if(selected==null)return;work(()->{
        JSONObject snapshot=selected.status();if(!background)selected.get("/api/station/playlists");JSONObject list=selected.playlist();JSONObject manifest=snapshot.optJSONObject("capabilities")!=null&&snapshot.getJSONObject("capabilities").optBoolean("smart_sharing")?selected.get("/api/smart/manifest"):null;
        activity.runOnUiThread(()->{if(!closed&&client==selected){if(!background)verified=true;render(snapshot,list);renderShared(manifest);}});return "Host status and access refreshed";
    },background,true);}
    private void command(boolean launch){final CastClient selected=client;if(selected==null||!verified||(launch&&!playlistSupported))return;work(()->{selected.command(launch);JSONObject snapshot=selected.status(),list=selected.playlist();activity.runOnUiThread(()->{if(!closed&&client==selected)render(snapshot,list);});return launch?"Station started":"Station stopped";});}
    private void render(JSONObject snapshot,JSONObject list){
        JSONObject capabilities=snapshot.optJSONObject("capabilities");
        hostLaunchSupported=capabilities!=null&&capabilities.optBoolean("host_master_launch",false);
        serverOutputSupported=capabilities!=null&&capabilities.optBoolean("server_output_control",false);
        playlistSupported=capabilities!=null&&capabilities.optBoolean("playlist_playback",false);
        serverOutputRunning=snapshot.optBoolean("server_output",false);
        capabilityNote.setText("Desktop host broadcast: "+(hostLaunchSupported?"command supported; desktop readiness checked when requested":"not advertised; update/check selected host")+"\nServer speakers: "+(serverOutputSupported?"command supported; physical output unverified":"not advertised; update/check selected host"));
        running=snapshot.optBoolean("running");onAir=snapshot.optBoolean("on_air",running);preparing=snapshot.optBoolean("preparing",false);
        stationName=snapshot.optString("station_name","AMP Cast");
        now.setText(stationName+" · "+(snapshot.optBoolean("on_air",snapshot.optBoolean("running"))?"ON AIR":snapshot.optBoolean("preparing")?"Preparing audio":"Stopped")+"\n"+(snapshot.isNull("now_playing")?"No track playing":snapshot.optString("now_playing",""))+"\nSource: "+snapshot.optString("current_source",snapshot.optString("source","unknown"))+" · Server output: "+(serverOutputRunning?"running":"off")+" · Password required: "+snapshot.optBoolean("password_required")+"\nListeners: "+snapshot.optInt("listeners")+"\n"+(client==null?"":client.protection())+(snapshot.isNull("error")?"":"\n"+snapshot.optString("error","")));
        tracks=list.optJSONArray("tracks");if(tracks==null)tracks=new JSONArray();renderQueue();
    }
    private void renderShared(JSONObject manifest){
        sharedLists.removeAllViews();final CastClient owner=client;if(manifest==null){if(client!=null){sharedLists.addView(label("Playlist downloads are off on this host. Live listening does not save songs or cues.",14));sharedLists.addView(action("Enable playlist downloads…",()->new android.app.AlertDialog.Builder(activity).setTitle("Share this host's playlists?").setMessage("Allow connected devices with host access to download playlist audio and prepared cues. This does not change the live DJ broadcast.").setPositiveButton("Enable",(d,n)->{final CastClient selected=owner;if(!selected(selected))return;work(()->{selected.post("/api/smart/enable",null);JSONObject available=selected.get("/api/smart/manifest");activity.runOnUiThread(()->{if(!closed&&client==selected)renderShared(available);});return "Choose a playlist below, then Save offline with my cues.";});}).setNegativeButton("Cancel",null).show()));}return;}queue.setVisibility(GONE);sharedLists.addView(label("Station playlists",18));JSONArray lists=manifest.optJSONArray("playlists");if(lists==null)return;
        for(int i=0;i<lists.length();i++){JSONObject playlist=lists.optJSONObject(i);if(playlist==null)continue;sharedLists.addView(label(playlist.optString("name")+" · "+playlist.optJSONArray("tracks").length()+" songs",16));final CastClient selected=client;if(playlistSupported)sharedLists.addView(action("Use playlist as host source",()->{if(!selected(selected)||!playlistSupported)return;work(()->{selected.playPlaylist(playlist.optString("id"));sync(selected);return "Host playlist requested. Choose a destination when it is on air.";});}));sharedLists.addView(action("Save offline with my cues",()->downloadPlaylist(selected,manifest,playlist)));}
    }
    private void downloadPlaylist(CastClient selected,JSONObject manifest,JSONObject playlist){
        if(!selected(selected))return;
        java.util.concurrent.atomic.AtomicBoolean cancel=new java.util.concurrent.atomic.AtomicBoolean();
        android.widget.ProgressBar bar=new android.widget.ProgressBar(activity,null,android.R.attr.progressBarStyleHorizontal);bar.setMax(Math.max(1,playlist.optJSONArray("tracks")==null?1:playlist.optJSONArray("tracks").length()));
        LinearLayout content=new LinearLayout(activity);content.setOrientation(VERTICAL);TextView detail=label("Preparing download…",16);content.addView(detail);content.addView(bar);
        android.app.AlertDialog dialog=new android.app.AlertDialog.Builder(activity).setTitle("Save "+playlist.optString("name","playlist")+" offline").setView(content).setNegativeButton("Cancel",null).create();
        dialog.setCancelable(false);dialog.show();dialog.getButton(-2).setOnClickListener(v->{cancel.set(true);detail.setText("Cancelling; existing downloads are kept…");dialog.getButton(-2).setEnabled(false);});
        work(()->{try{
            selected.status();
            JSONObject latest=selected.get("/api/smart/manifest"),chosen=null;JSONArray available=latest.getJSONArray("playlists");for(int i=0;i<available.length();i++){JSONObject candidate=available.getJSONObject(i);if(!playlist.optString("id").isEmpty()?playlist.optString("id").equals(candidate.optString("id")):playlist.optString("name").equals(candidate.optString("name"))){chosen=candidate;break;}}
            if(chosen==null)throw new java.io.IOException("This playlist is no longer shared. Your saved offline copy was kept.");
            java.io.File folder=RadioOffline.save(activity,selected,latest,chosen,new RadioOffline.Progress(){
                public boolean cancelled(){return cancel.get()||closed||Thread.currentThread().isInterrupted();}
                public void song(int i,int total,String title){activity.runOnUiThread(()->{if(!closed&&!cancel.get()){bar.setMax(Math.max(1,total));bar.setProgress(i-1);detail.setText(i+" / "+total+" · "+title+"\nAudio + prepared cues");}});}
            });
            String name=RadioOffline.album(folder).optString("name");String syncSummary=RadioOffline.summary(folder);
            activity.runOnUiThread(()->{if(!closed)new android.app.AlertDialog.Builder(activity).setTitle("Available offline").setMessage(name+"\n"+syncSummary).setNeutralButton("Review sync changes",(d,n)->RadioOffline.reviewConflicts(activity,folder)).setPositiveButton("Open offline playlists",(d,n)->offline()).setNegativeButton("Done",null).show();});
            return name+": "+syncSummary;
        }catch(Exception e){if(cancel.get())return "Download cancelled. Existing offline playlists were kept.";throw e;}
        finally{activity.runOnUiThread(dialog::dismiss);}});
    }
    private void chooseStationPlaylist(){final CastClient selected=client;if(!selected(selected)||!playlistSupported){status.setText("Connect and verify host access first");return;}work(()->{JSONArray lists=selected.get("/api/station/playlists").getJSONArray("playlists");activity.runOnUiThread(()->{if(closed||client!=selected)return;if(lists.length()==0){status.setText("No saved host playlists available");return;}String[] names=new String[lists.length()];for(int i=0;i<names.length;i++)names[i]=lists.optJSONObject(i).optString("name");new android.app.AlertDialog.Builder(activity).setTitle("Launch independent Smart Radio").setItems(names,(d,i)->{if(!selected(selected)||!playlistSupported)return;work(()->{selected.post("/api/station/play/"+android.net.Uri.encode(lists.optJSONObject(i).optString("id")),null);sync(selected);return "Smart Radio launch requested";});}).setNegativeButton("Cancel",null).show();});return "Choose a saved playlist";});}
    private void offline(){java.util.List<java.io.File> albums=RadioOffline.albums(activity);String[] names=new String[albums.size()];for(int i=0;i<names.length;i++)try{names[i]=RadioOffline.album(albums.get(i)).optString("name");}catch(Exception error){names[i]="Unavailable playlist";}if(names.length==0){new android.app.AlertDialog.Builder(activity).setTitle("No offline playlists yet").setMessage("Listening to a live broadcast does not download its songs. Connect to your AMP Cast host, enable playlist downloads, then choose Save offline with my cues beside a playlist. The songs and analysis must finish downloading before they appear here.").setPositiveButton(client==null?"Find stations":"Show downloadable playlists",(d,n)->{if(client==null)findServers();else{refresh();sharedLists.requestRectangleOnScreen(new android.graphics.Rect(0,0,sharedLists.getWidth(),sharedLists.getHeight()),false);}}).setNegativeButton("Close",null).show();return;}new android.app.AlertDialog.Builder(activity).setTitle("Offline station playlists").setItems(names,(d,index)->{try{listener.playlist(RadioOffline.items(albums.get(index)));}catch(Exception error){status.setText(error.getMessage());}}).setNegativeButton("Close",null).show();}
    private void renderQueue(){
        String query=search.getText().toString().trim().toLowerCase(java.util.Locale.ROOT);StringBuilder text=new StringBuilder();int total=0,shown=0;
        for(int i=0;i<tracks.length();i++){JSONObject track=tracks.optJSONObject(i);if(track==null)continue;String title=track.optString("title","Track");if(!title.toLowerCase(java.util.Locale.ROOT).contains(query))continue;total++;if(shown++<200)text.append(i+1).append(". ").append(title).append('\n');}
        if(total>200)text.append("Showing 200 of ").append(total).append(" matches. Refine your search.");
        queue.setText(text.length()==0?(client==null?"Find a server to see its music.":query.isEmpty()?"No songs in this station yet.":"No songs match your search."):text.toString());
    }
    private void findServers(){
        work(()->{java.util.List<CastDiscovery.Server> servers=CastDiscovery.find();activity.runOnUiThread(()->{if(closed)return;serverList.removeAllViews();for(CastDiscovery.Server server:servers){serverList.addView(action(server.name+" · "+server.url.replace("http://",""),()->{address.setText(server.url);password.setText("");connect(server.id);}));}});return servers.isEmpty()?"No LAN hosts found. Check Wi-Fi or enter a LAN address. A listener bound to 127.0.0.1 cannot accept phone connections; do not change a live server during analysis.":"Choose a server below.";},false,false);
    }
    void close(){closed=true;handler.removeCallbacksAndMessages(null);worker.shutdownNow();}
}
