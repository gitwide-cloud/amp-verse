package cloud.gitwide.ampverse;

import android.app.Activity;
import android.content.Context;
import android.text.InputType;
import android.widget.*;
import org.json.*;
import java.util.concurrent.*;

/** Integrated Android station controls; local listening uses the existing playback service. */
class CastPanel extends LinearLayout {
    interface Listener {void listen(String url,String name);void stopListening();default boolean phoneAvailable(){return true;}default void playlist(java.util.List<androidx.media3.common.MediaItem> items){}}
    final Activity activity;
    final Appearance colors;
    final Listener listener;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    final TextView status,now,queue;
    private TextView capabilityNote,connectionState;
    private Button stopPhone,disconnect,chromecast,hostBroadcast,serverPlay,serverStop,stationPlaylist;
    private volatile boolean verified;
    private boolean hostLaunchSupported,serverOutputSupported,serverOutputRunning;
    boolean playlistSupported,sharingSupported;
    private long foregroundGeneration;
    final EditText address,password,search;
    final LinearLayout serverList,connectionFields,sharedLists;
    private LinearLayout stationControls;
    private final CastLibrary library;
    private org.json.JSONArray tracks=new org.json.JSONArray();
    private Button find;
    private final Button connect,start,stop,listen,refresh;
    CastClient client;
    boolean closed,busy,polling,running,onAir,preparing;
    private String stationName="AMP Cast";
    private final android.os.Handler handler=new android.os.Handler(android.os.Looper.getMainLooper());
    private final Runnable poll=new Runnable(){public void run(){if(closed)return;if(client!=null&&!busy&&!polling&&verified)refresh(true);handler.postDelayed(this,3000);}};
    CastPanel(Activity activity,Appearance colors,Listener listener) {
        super(activity);this.activity=activity;this.colors=colors;this.listener=listener;
        setOrientation(VERTICAL);setPadding(dp(10),dp(10),dp(10),dp(10));setBackground(Chrome.panel(colors,false));
        addView(label("Choose AMP Radio",20));
        addView(label("Discovery and manual connection only select a host. They do not start or stop playback.",14));
        find=action("Find nearby servers",this::findServers);addView(find);
        serverList=new LinearLayout(activity);serverList.setOrientation(VERTICAL);addView(serverList);
        int broadcastStart=getChildCount();
        LinearLayout radioResults=new LinearLayout(activity);radioResults.setOrientation(VERTICAL);
        EditText radioQuery=new EditText(activity);radioQuery.setSingleLine(true);radioQuery.setHint("Search broadcast stations by name");radioQuery.setTextColor(colors.text);radioQuery.setHintTextColor(colors.muted);addView(radioQuery);
        addView(action("Search broadcast stations",()->{final String query=radioQuery.getText().toString().trim();if(query.isEmpty())return;work(()->{JSONArray stations=RadioDirectory.search(query);activity.runOnUiThread(()->{if(closed)return;radioResults.removeAllViews();for(int i=0;i<stations.length();i++){JSONObject station=stations.optJSONObject(i);if(station==null)continue;String stream=station.optString("url_resolved");if(!stream.startsWith("http://")&&!stream.startsWith("https://"))continue;radioResults.addView(action(station.optString("name")+" · Listen",()->listener.listen(stream,station.optString("name"))));}});return "Broadcast station results";},false,false);}));
        addView(radioResults);UiSections.fold(this,colors,"Internet radio",broadcastStart);
        connectionFields=new LinearLayout(activity);connectionFields.setOrientation(VERTICAL);
        Button manual=action("Advanced · server address and password",()->connectionFields.setVisibility(connectionFields.getVisibility()==VISIBLE?GONE:VISIBLE));addView(manual);
        addView(connectionFields);connectionFields.setVisibility(GONE);
        address=field("http://192.168.1.50:8787",false);address.setContentDescription("AMP Cast server address");address.setText(activity.getSharedPreferences("settings",Context.MODE_PRIVATE).getString("cast-server",""));
        password=field("Optional admin password",true);password.setContentDescription("AMP Cast admin password");
        connect=action("Connect",()->connect());connectionFields.addView(connect);
        connectionState=label("Server connection: disconnected",16);connectionFields.addView(connectionState);
        status=label("Not connected. A host bound to 127.0.0.1 is reachable only on that host. Use a LAN-enabled host.",14);addView(status);
        now=label("No station selected",18);addView(now);
        refresh=action("Refresh host status",()->refresh());addView(refresh);
        disconnect=action("Disconnect from server",this::disconnect);addView(disconnect);
        library=new CastLibrary(this);addView(library);
        stationControls=new LinearLayout(activity);stationControls.setOrientation(VERTICAL);addView(stationControls);
        stationControls.addView(label("Outputs · hear the host’s current stream",18));
        stationControls.addView(label("These destinations receive the selected host’s program, not this phone’s music. Buffered output may be delayed.",14));
        listen=action("Listen on this phone",()->{if(client!=null)listener.listen(client.phoneStream(),stationName);});stationControls.addView(listen);
        stopPhone=action("Stop phone listening",listener::stopListening);addView(stopPhone,indexOfChild(stationControls));
        chromecast=action("Google Cast",()->{if(client==null||!onAir){status.setText("Connect and start a station or live DJ broadcast first.");return;}activity.startActivity(new android.content.Intent(activity,GoogleCastActivity.class).putExtra("stream",client.stream()).putExtra("title",stationName));});
        serverPlay=action("Play on server speakers",()->{final CastClient selected=client;if(selected!=null&&verified&&serverOutputSupported)work(()->{selected.post("/api/output/start",null);sync(selected);return "Server output requested. Audio device availability and audibility are not confirmed.";});});stationControls.addView(serverPlay);stationControls.addView(chromecast);
        serverStop=action("Stop server speaker only",()->{final CastClient selected=client;if(selected!=null&&verified&&serverOutputSupported)work(()->{selected.post("/api/output/stop",null);sync(selected);return "Server speaker stop requested; station continues.";});});stationControls.addView(serverStop);
        int sourceStart=stationControls.getChildCount();
        stationControls.addView(label("Source · change the selected host’s program",18));
        stationControls.addView(label("Source controls affect everyone listening to this host. Choosing a destination above does not change its source.",14));
        LinearLayout controls=new LinearLayout(activity);controls.setOrientation(VERTICAL);start=action("Start host station",()->command(true));stop=action("Stop host program for all listeners",()->command(false));controls.addView(start,new LinearLayout.LayoutParams(-1,-2));controls.addView(stop,new LinearLayout.LayoutParams(-1,-2));stationControls.addView(controls);
        stationPlaylist=action("Choose independent Smart Radio playlist…",this::chooseStationPlaylist);stationControls.addView(stationPlaylist);
        hostBroadcast=action("Request desktop host Player/DJ master…",()->{final CastClient selected=client;if(selected==null||!verified||!hostLaunchSupported)return;EditText secret=new EditText(activity);secret.setHint("Desktop Player/DJ control password, if configured");secret.setInputType(129);new android.app.AlertDialog.Builder(activity).setTitle("Change host source to desktop master?").setMessage("This requests the Player/DJ on the host machine. It does not send phone audio. The desktop must be running with its control endpoint enabled; command support does not confirm it is reachable. Existing station listeners will hear the new program.").setView(secret).setPositiveButton("Request host master",(d,n)->{if(client!=selected||!verified||busy)return;final String value=secret.getText().toString();work(()->{selected.post("/api/live/launch",new JSONObject().put("source_password",value));sync(selected);return "Host sender requested; waiting for master audio. Phone audio was not sent.";});}).setNegativeButton("Cancel",null).show();});stationControls.addView(hostBroadcast);
        Button phoneSend=action("Send this phone’s current playback",()->{});phoneSend.setEnabled(false);phoneSend.setAlpha(.35f);stationControls.addView(phoneSend);
        stationControls.addView(label("Phone-source sending is not implemented in this Android build. Station listening and Chromecast forwarding do not capture the phone Player or DJ master.",14));
        UiSections.fold(stationControls,colors,"Source actions · change host program",sourceStart);
        capabilityNote=label("Host capabilities have not been checked.",14);stationControls.addView(capabilityNote);
        int extrasStart=getChildCount();
        addView(action("Google Cast receiver controls",()->activity.startActivity(new android.content.Intent(activity,GoogleCastActivity.class))));
        sharedLists=new LinearLayout(activity);sharedLists.setOrientation(VERTICAL);addView(sharedLists);


        search=new EditText(activity);search.setSingleLine(true);search.setTextColor(colors.text);search.setHintTextColor(colors.muted);search.setHint("Search songs in this station");search.setContentDescription("Search station songs");addView(search);
        queue=label("Find a server to see its music.",14);addView(queue);
        search.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int a,int c,int d){}public void onTextChanged(CharSequence t,int a,int b,int c){renderQueue();}public void afterTextChanged(android.text.Editable t){}});
        UiSections.fold(this,colors,"Advanced · station downloads and diagnostics",extrasStart);
        addView(action("Help",()->new android.app.AlertDialog.Builder(activity).setTitle("Radio help").setMessage("Find nearby servers, choose AMP Radio, and open Library. Create saves on the server; Download makes a one-way offline copy. Remote opens that server’s web player. Two-way sync is not implemented.").setPositiveButton("Done",null).show()));enabled(false);
        android.view.View heading=getChildAt(0);
        android.view.View[] primary={heading,find,serverList,connectionState,status,now,disconnect,listen,stopPhone,library};
        LinearLayout advanced=new LinearLayout(activity);advanced.setOrientation(VERTICAL);
        for(android.view.View view:primary)if(view.getParent() instanceof android.view.ViewGroup)((android.view.ViewGroup)view.getParent()).removeView(view);
        while(getChildCount()>0){android.view.View child=getChildAt(0);removeView(child);advanced.addView(child);}
        for(android.view.View view:primary)addView(view);
        int advancedStart=getChildCount();addView(advanced);UiSections.fold(this,colors,"Advanced",advancedStart);
        handler.postDelayed(poll,3000);
    }
    private int dp(int n){return (int)(n*getResources().getDisplayMetrics().density);}
    TextView label(String text,int size){TextView v=new TextView(activity);v.setText(text);v.setTextColor(colors.text);v.setTextSize(size);v.setPadding(0,dp(6),0,dp(6));return v;}
    private EditText field(String hint,boolean secret){EditText v=new EditText(activity);v.setHint(hint);v.setSingleLine(true);v.setTextColor(colors.text);v.setHintTextColor(colors.muted);v.setInputType(InputType.TYPE_CLASS_TEXT|(secret?InputType.TYPE_TEXT_VARIATION_PASSWORD:InputType.TYPE_TEXT_VARIATION_URI));connectionFields.addView(v);return v;}
    Button action(String text,Runnable action){Button b=new Button(activity);b.setText(text);b.setAllCaps(false);b.setTextColor(Chrome.ink(colors,false));b.setBackground(Chrome.button(colors,false));b.setMinHeight(dp(48));b.setOnClickListener(v->action.run());return b;}
    private void enabled(boolean connected){
        listen.setVisibility(connected?VISIBLE:GONE);
        stationControls.setVisibility(connected?VISIBLE:GONE);search.setVisibility(connected?VISIBLE:GONE);queue.setVisibility(connected?VISIBLE:GONE);
        connectionState.setText("Server connection: "+(busy?"checking / updating":connected&&verified?"verified · "+stationName:connected?"access or connection lost — re-verify":"disconnected"));
        boolean ready=connected&&verified&&!busy;library.availability(ready);
        start.setEnabled(ready&&playlistSupported&&!running&&!preparing);stop.setEnabled(ready&&(running||preparing));listen.setEnabled(ready&&onAir&&listener.phoneAvailable());stopPhone.setEnabled(listener.phoneAvailable());disconnect.setEnabled(connected&&!busy);chromecast.setEnabled(ready&&onAir);
        hostBroadcast.setEnabled(ready&&hostLaunchSupported);serverPlay.setEnabled(ready&&serverOutputSupported&&onAir&&!serverOutputRunning);serverStop.setEnabled(ready&&serverOutputSupported&&serverOutputRunning);stationPlaylist.setEnabled(ready&&playlistSupported);
        setChildrenEnabled(sharedLists,ready);
        refresh.setEnabled(connected&&!busy);connect.setEnabled(!busy);find.setEnabled(!busy);address.setEnabled(!busy);password.setEnabled(!busy);
        for(Button button:new Button[]{start,stop,listen,stopPhone,disconnect,refresh,chromecast,hostBroadcast,serverPlay,serverStop,stationPlaylist})button.setAlpha(button.isEnabled()?1f:.35f);
    }
    private void setChildrenEnabled(android.view.View view,boolean value){
        view.setEnabled(value);if(view instanceof Button)view.setAlpha(value?1f:.35f);
        if(view instanceof android.view.ViewGroup){android.view.ViewGroup group=(android.view.ViewGroup)view;for(int i=0;i<group.getChildCount();i++)setChildrenEnabled(group.getChildAt(i),value);}
    }
    interface Operation {String run()throws Exception;}
    void work(Operation operation){work(operation,false,true);}
    void work(Operation operation,boolean background,boolean affectsHost){
        if(closed||busy||(background&&polling))return;
        final CastClient context=client;
        final long generation=background?foregroundGeneration:++foregroundGeneration;
        if(background)polling=true;else{busy=true;enabled(client!=null);status.setText("Working…");}
        worker.execute(()->{String message;boolean failed=false,unavailable=false;
            try{message=operation.run();}catch(Exception error){failed=true;unavailable=affectsHost&&(background||error instanceof CastClient.AuthorizationException||error instanceof CastClient.HostUnavailableException);message=(unavailable?"Server access unavailable: ":"Action could not complete: ")+(error.getMessage()==null?error.getClass().getSimpleName():error.getMessage())+(unavailable?(context==null?" Select the server again or check Advanced.":" Refresh host status to re-verify."):"");}
            final String result=message;final boolean error=failed,lost=unavailable;
            activity.runOnUiThread(()->{if(closed)return;if(background)polling=false;else busy=false;
                if(background&&(client!=context||generation!=foregroundGeneration))return;
                if(lost&&client==context)verified=false;
                if(!background||error)status.setText(result);enabled(client!=null);
            });
        });
    }
    boolean selected(CastClient candidate){return candidate!=null&&client==candidate&&verified&&!busy&&!closed;}
    void sync(CastClient selected)throws Exception{
        JSONObject snapshot=selected.status(),list=selected.playlist();
        activity.runOnUiThread(()->{if(!closed&&client==selected)render(snapshot,list);});
    }
    private void connect(){connect(null);}
    private void connect(String expectedIdentity){
        if(busy||closed)return;
        final String url=address.getText().toString(),secret=password.getText().toString();library.clear();client=null;verified=false;running=false;stationName="AMP Radio";sharingSupported=false;hostLaunchSupported=false;serverOutputSupported=false;playlistSupported=false;serverOutputRunning=false;onAir=false;preparing=false;now.setText("No host selected");capabilityNote.setText("Host capabilities have not been checked.");tracks=new JSONArray();sharedLists.removeAllViews();enabled(false);
        work(()->{CastClient.requireLanAddress(url);CastClient candidate=new CastClient(url,secret);JSONObject snapshot=candidate.connect();if(expectedIdentity!=null&&!expectedIdentity.equals(snapshot.optString("server_id")+"/"+snapshot.optString("station_id")))throw new java.io.IOException("Discovered host identity changed. Select the host again.");if(snapshot.optBoolean("password_required")&&secret.isEmpty()){activity.runOnUiThread(()->{if(!closed)requestPassword(snapshot.optString("station_name","AMP Radio"),expectedIdentity);});return "This server’s Library is password-protected.";}candidate.get("/api/station/playlists");JSONObject libraryData=CastLibrary.snapshot(candidate);JSONObject list=candidate.playlist();JSONObject manifest=snapshot.optJSONObject("capabilities")!=null&&snapshot.getJSONObject("capabilities").optBoolean("smart_sharing")?candidate.get("/api/smart/manifest"):null;
            activity.runOnUiThread(()->{if(closed)return;client=candidate;verified=true;connectionFields.setVisibility(GONE);activity.getSharedPreferences("settings",Context.MODE_PRIVATE).edit().putString("cast-server",candidate.base).apply();render(snapshot,list);renderShared(manifest);library.open(candidate,stationName,libraryData);});return "Server connected and access verified. Choose an output; existing station playback is unchanged.";});
    }
    void refresh(){refresh(false);}
    private void refresh(boolean background){final CastClient selected=client;if(selected==null)return;work(()->{
        JSONObject snapshot=selected.status();if(!background)selected.get("/api/station/playlists");JSONObject list=selected.playlist();JSONObject manifest=snapshot.optJSONObject("capabilities")!=null&&snapshot.getJSONObject("capabilities").optBoolean("smart_sharing")?selected.get("/api/smart/manifest"):null;
        activity.runOnUiThread(()->{if(!closed&&client==selected){if(!background)verified=true;render(snapshot,list);renderShared(manifest);}});return "Server connection and access verified";
    },background,true);}
    private void command(boolean launch){final CastClient selected=client;if(selected==null||!verified||(launch&&!playlistSupported))return;work(()->{selected.command(launch);JSONObject snapshot=selected.status(),list=selected.playlist();activity.runOnUiThread(()->{if(!closed&&client==selected)render(snapshot,list);});return launch?"Station started":"Station stopped";});}
    private void render(JSONObject snapshot,JSONObject list){
        JSONObject capabilities=snapshot.optJSONObject("capabilities");
        sharingSupported=capabilities!=null&&capabilities.optBoolean("smart_sharing",false);
        hostLaunchSupported=capabilities!=null&&capabilities.optBoolean("host_master_launch",false);
        serverOutputSupported=capabilities!=null&&capabilities.optBoolean("server_output_control",false);
        playlistSupported=capabilities!=null&&capabilities.optBoolean("playlist_playback",false);
        serverOutputRunning=snapshot.optBoolean("server_output",false);
        capabilityNote.setText("Desktop host broadcast: "+(hostLaunchSupported?"command supported; desktop readiness checked when requested":"not advertised; update/check selected host")+"\nServer speakers: "+(serverOutputSupported?"command supported; physical output unverified":"not advertised; update/check selected host"));
        running=snapshot.optBoolean("running");onAir=snapshot.optBoolean("on_air",running);preparing=snapshot.optBoolean("preparing",false);
        stationName=snapshot.optString("station_name","AMP Cast");
        String source=snapshot.isNull("current_source")?snapshot.optString("source",""):snapshot.optString("current_source","");
        if(source.trim().isEmpty())source="none";
        now.setText(stationName+" · "+(onAir?"On air":preparing?"Preparing audio":"Idle — select a playlist in Library")+"\n"+(snapshot.isNull("now_playing")?"No track playing":snapshot.optString("now_playing",""))+"\nSource: "+source+(verified?"":" · connection not verified"));
        tracks=list.optJSONArray("tracks");if(tracks==null)tracks=new JSONArray();renderQueue();
    }
    private void renderShared(JSONObject manifest){
        sharedLists.removeAllViews();final CastClient owner=client;if(manifest==null){if(client!=null){sharedLists.addView(label("Playlist downloads are off on this host. Live listening does not save songs or cues.",14));sharedLists.addView(action("Enable playlist downloads…",()->new android.app.AlertDialog.Builder(activity).setTitle("Share this host's playlists?").setMessage("Allow connected devices with host access to download playlist audio and prepared cues. This does not change the live DJ broadcast.").setPositiveButton("Enable",(d,n)->{final CastClient selected=owner;if(!selected(selected))return;work(()->{selected.post("/api/smart/enable",null);JSONObject available=selected.get("/api/smart/manifest");activity.runOnUiThread(()->{if(!closed&&client==selected)renderShared(available);});return "Choose a playlist below, then Save offline with my cues.";});}).setNegativeButton("Cancel",null).show()));}return;}queue.setVisibility(GONE);sharedLists.addView(label("Station playlists",18));JSONArray lists=manifest.optJSONArray("playlists");if(lists==null)return;
        for(int i=0;i<lists.length();i++){JSONObject playlist=lists.optJSONObject(i);if(playlist==null)continue;sharedLists.addView(label(playlist.optString("name")+" · "+playlist.optJSONArray("tracks").length()+" songs",16));final CastClient selected=client;sharedLists.addView(action("Save offline with my cues",()->downloadPlaylist(selected,manifest,playlist)));}
    }
    void downloadPlaylist(CastClient selected,JSONObject manifest,JSONObject playlist){status.setText("Downloads require AMP Player.");}
    private void chooseStationPlaylist(){final CastClient selected=client;if(!selected(selected)||!playlistSupported){status.setText("Connect and verify host access first");return;}work(()->{JSONArray lists=selected.get("/api/station/playlists").getJSONArray("playlists");if(lists.length()==0)return "No saved host playlists available";activity.runOnUiThread(()->{if(closed||client!=selected)return;String[] names=new String[lists.length()];for(int i=0;i<names.length;i++)names[i]=lists.optJSONObject(i).optString("name");new android.app.AlertDialog.Builder(activity).setTitle("Launch independent Smart Radio").setItems(names,(d,i)->{if(!selected(selected)||!playlistSupported)return;work(()->{selected.post("/api/station/play/"+android.net.Uri.encode(lists.optJSONObject(i).optString("id")),null);sync(selected);return "Smart Radio launch requested";});}).setNegativeButton("Cancel",null).show();});return "Choose a saved playlist";});}
    void offline(){status.setText("Offline playlists require AMP Player.");}
    private void renderQueue(){
        String query=search.getText().toString().trim().toLowerCase(java.util.Locale.ROOT);StringBuilder text=new StringBuilder();int total=0,shown=0;
        for(int i=0;i<tracks.length();i++){JSONObject track=tracks.optJSONObject(i);if(track==null)continue;String title=track.optString("title","Track");if(!title.toLowerCase(java.util.Locale.ROOT).contains(query))continue;total++;if(shown++<200)text.append(i+1).append(". ").append(title).append('\n');}
        if(total>200)text.append("Showing 200 of ").append(total).append(" matches. Refine your search.");
        queue.setText(text.length()==0?(client==null?"Find a server to see its music.":query.isEmpty()?"No songs in this station yet.":"No songs match your search."):text.toString());
    }
    private void chooseServer(CastDiscovery.Server server){
        if(busy)return;address.setText(server.url);password.setText("");connect(server.id);
    }
    private void requestPassword(String name,String expectedIdentity){
        EditText secret=new EditText(activity);secret.setSingleLine(true);secret.setInputType(129);secret.setHint("Server password");secret.setContentDescription("Selected server password");
        new android.app.AlertDialog.Builder(activity).setTitle(name+" · protected Library").setMessage("Enter this server’s password to open Library.").setView(secret).setPositiveButton("Open Library",(d,n)->{if(busy||closed)return;password.setText(secret.getText());connect(expectedIdentity);}).setNegativeButton("Cancel",null).show();
    }
    void findServers(){
        work(()->{java.util.List<CastDiscovery.Server> servers=CastDiscovery.find();activity.runOnUiThread(()->{if(closed)return;serverList.removeAllViews();for(CastDiscovery.Server server:servers){serverList.addView(action(server.name,()->{chooseServer(server);}));}});return servers.isEmpty()?"No LAN hosts found. Check Wi-Fi or enter a LAN address. A listener bound to 127.0.0.1 cannot accept phone connections; do not change a live server during analysis.":"Choose a server below.";},false,false);
    }
    void refreshAvailability(){enabled(client!=null);}
    void disconnect(){if(busy)return;library.clear();client=null;verified=false;running=false;onAir=false;preparing=false;tracks=new JSONArray();sharedLists.removeAllViews();hostLaunchSupported=false;serverOutputSupported=false;playlistSupported=false;serverOutputRunning=false;stationName="AMP Radio";capabilityNote.setText("No server selected");now.setText("No station selected");status.setText("Server disconnected. Existing station and playback outputs were left running. Stop phone listening separately if wanted.");enabled(false);}
    void close(){closed=true;handler.removeCallbacksAndMessages(null);worker.shutdownNow();}
}
