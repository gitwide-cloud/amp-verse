package cloud.gitwide.ampverse;

import android.app.Activity;
import android.content.Context;
import android.text.InputType;
import android.widget.*;
import org.json.*;
import java.util.concurrent.*;

/** Integrated Android station controls; local listening uses the existing playback service. */
class CastPanel extends LinearLayout {
    interface Listener {
        default boolean phoneSending(){return false;}
        default boolean phoneSenderAvailable(){return false;}
        default String phoneBroadcastStatus(){return "Phone master sending requires the Full Player/DJ host.";}
        default String startPhoneBroadcast(CastClient selected){return "Open the Full Player/DJ host to send its master.";}
        default void stopPhoneBroadcast(){}
        default boolean phoneListening(){return false;}default void remote(){}default boolean phoneAvailable(){return true;}default boolean playlistSyncAvailable(){return false;}default void playlistSync(CastClient selected){}void listen(String url,String name);void stopListening();default void playlist(java.util.List<androidx.media3.common.MediaItem> items){}}
    final Activity activity;
    private final Appearance colors;
    final Listener listener;
    private final CastHostSession hostSession;
    private android.net.ConnectivityManager connectivity;
    private android.net.ConnectivityManager.NetworkCallback networkCallback;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    final TextView status,now,queue;
    private TextView capabilityNote,diagnostics,serverSummary,stationSummary;
    private LinearLayout dashboardStations;
    private Button selectedStation;
    private Button chromecast,hostBroadcast,serverPlay,serverStop,stationPlaylist,phoneSend,phoneStop;
    private TextView phoneSendNote;
    private volatile boolean verified;
    private boolean hostLaunchSupported,serverOutputSupported,playlistSupported,serverOutputRunning;
    final EditText address,password,search;
    final LinearLayout serverList,connectionFields,sharedLists;
    private LinearLayout stationControls;
    private CastLibraryView libraryView;
    private org.json.JSONArray tracks=new org.json.JSONArray();
    private Button find;
    private final Button connect,start,stop,listen,refresh;
    volatile CastClient client;
    boolean closed,busy,polling,running,preparing,passwordRequired,authRejected,sharingVisible,pollFailed;
    private long operationVersion;
    private String hostError="",sharedFingerprint="";
    private TextView actionFeedback,feedbackForActions,recoveryFeedback,sharedFeedback;
    private TextView localNote,castNote,outputNote,sourceNote,desktopNote;
    private final java.util.List<TextView> feedbackViews=new java.util.ArrayList<>();
    private String stationName="AMP Cast";
    private final android.os.Handler handler=new android.os.Handler(android.os.Looper.getMainLooper());
    private final Runnable poll=new Runnable(){public void run(){if(closed)return;if(!busy&&!polling){if(client!=null&&verified)refresh(false);else resumeConnection();}handler.postDelayed(this,3000);}};
    CastPanel(Activity activity,Appearance colors,Listener listener) {
        super(activity);this.activity=activity;this.colors=colors;this.listener=listener;this.hostSession=CastHostSession.get(activity);
        setOrientation(VERTICAL);setPadding(dp(10),dp(10),dp(10),dp(10));setBackground(Chrome.panel(colors,false));
        addView(label("AMP Radio · choose a server",20));
        addView(label("Discovery and manual connection only select a host. They do not start or stop playback.",14));
        find=action("Find nearby stations",this::findServers);addView(find);
        serverList=new LinearLayout(activity);serverList.setOrientation(VERTICAL);addView(serverList);
        int broadcastStart=getChildCount();
        LinearLayout radioResults=new LinearLayout(activity);radioResults.setOrientation(VERTICAL);
        EditText radioQuery=new EditText(activity);radioQuery.setSingleLine(true);radioQuery.setHint("Search broadcast stations by name");radioQuery.setTextColor(colors.text);radioQuery.setHintTextColor(colors.muted);addView(radioQuery);
        addView(action("Search broadcast stations",()->{final String query=radioQuery.getText().toString().trim();if(query.isEmpty())return;work(()->{JSONArray stations=RadioDirectory.search(query);activity.runOnUiThread(()->{if(closed)return;radioResults.removeAllViews();for(int i=0;i<stations.length();i++){JSONObject station=stations.optJSONObject(i);if(station==null)continue;String stream=station.optString("url_resolved");if(!stream.startsWith("http://")&&!stream.startsWith("https://"))continue;radioResults.addView(action(station.optString("name")+" · Listen",()->listener.listen(stream,station.optString("name"))));}});return "Broadcast station results";});}));
        addView(radioResults);
        UiSections.fold(this,colors,"Internet radio · tap a result to listen",broadcastStart);
        connectionFields=new LinearLayout(activity);connectionFields.setOrientation(VERTICAL);
        Button manual=action("Advanced connection settings",()->connectionFields.setVisibility(connectionFields.getVisibility()==VISIBLE?GONE:VISIBLE));addView(manual);
        addView(connectionFields);connectionFields.setVisibility(GONE);
        address=field("http://192.168.1.50:8787",false);address.setContentDescription("AMP Cast server address");address.setText(activity.getSharedPreferences("settings",Context.MODE_PRIVATE).getString("cast-server",""));
        password=field("Optional admin password",true);password.setContentDescription("AMP Cast admin password");
        connect=action("Connect",()->connect());connectionFields.addView(connect);connectionFields.addView(action("Disconnect selected host",this::disconnectHost));diagnostics=label("",14);connectionFields.addView(diagnostics);
        status=label("Not connected. Find a nearby AMP host, or open Advanced connection settings to connect manually.",14);addView(status);
        now=label("No station selected",18);addView(now);
        libraryView=new CastLibraryView(activity,colors,listener,()->verified?client:null,this::downloadSaved);addView(libraryView);
        stationControls=new LinearLayout(activity);stationControls.setOrientation(VERTICAL);addView(stationControls);
        localNote=path("Local listening · host → this phone");
        stationControls.addView(label("These destinations receive the selected host’s program, not this phone’s music. Buffered output may be delayed.",14));
        listen=action("Listen to host on this phone",()->{if(listener.phoneSending()){report("Stop this phone’s broadcast before switching it to station listening.");return;}if(client!=null)listener.listen(client.phoneStream(),stationName);});stationControls.addView(listen);
        stationControls.addView(action("Stop phone playback",listener::stopListening));
        castNote=path("Google Cast · host → Google speaker");
        chromecast=action("Send host stream to Chromecast",()->{if(client==null||!running){status.setText("Connect and start a station or live DJ broadcast first.");return;}activity.startActivity(new android.content.Intent(activity,GoogleCastActivity.class).putExtra("stream",client.stream()).putExtra("title",stationName));});stationControls.addView(chromecast);
        outputNote=path("Server speakers · host → its audio output");
        serverPlay=action("Request playback on server speakers",()->{final CastClient selected=client;if(selected!=null&&verified&&serverOutputSupported)work(()->{postAndRefresh(selected,"/api/output/start",null);return "Server output requested. Audio device availability and audibility are not confirmed.";});});stationControls.addView(serverPlay);
        serverStop=action("Stop server speaker only",()->{final CastClient selected=client;if(selected!=null&&verified&&serverOutputSupported)work(()->{postAndRefresh(selected,"/api/output/stop",null);return "Server speaker stop requested; station continues.";});});stationControls.addView(serverStop);
        sourceNote=path("Station source · change the host’s program");
        stationControls.addView(label("Source controls affect everyone listening to this host. Choosing a destination above does not change its source.",14));
        LinearLayout controls=new LinearLayout(activity);start=action("Start host station",()->command(true));stop=action("Stop host program for all listeners",()->command(false));controls.addView(start,new LinearLayout.LayoutParams(0,dp(60),1));controls.addView(stop,new LinearLayout.LayoutParams(0,dp(60),1));stationControls.addView(controls);
        refresh=action("Refresh host status",()->refresh());stationControls.addView(refresh);
        int stationAdvanced=stationControls.getChildCount();
        stationPlaylist=action("Choose independent Smart Radio playlist…",this::chooseStationPlaylist);stationControls.addView(stationPlaylist);
        UiSections.fold(stationControls,colors,"Station controls",stationAdvanced);
        desktopNote=path("Remote desktop broadcast · desktop master → host");
        hostBroadcast=action("Request desktop host Player/DJ master…",()->{final CastClient selected=client;if(selected==null||!verified||!hostLaunchSupported)return;EditText secret=new EditText(activity);secret.setHint("Desktop Player/DJ control password, if configured");secret.setInputType(129);new android.app.AlertDialog.Builder(activity).setTitle("Change host source to desktop master?").setMessage("This requests the Player/DJ on the host machine. It does not send phone audio. The desktop must be running with its control endpoint enabled; command support does not confirm it is reachable. Existing station listeners will hear the new program.").setView(secret).setPositiveButton("Request host master",(d,n)->{if(!selected(selected))return;final String value=secret.getText().toString();work(()->{postAndRefresh(selected,"/api/live/launch",new JSONObject().put("source_password",value));return "Host sender requested. Check Source and ON AIR status for readiness. Phone audio was not sent.";});}).setNegativeButton("Cancel",null).show();});stationControls.addView(hostBroadcast);
        phoneSend=action("Send this phone’s current playback",()->{
            final CastClient selected=client;if(selected==null||!selected(selected))return;
            new android.app.AlertDialog.Builder(activity).setTitle("Send this phone’s master to "+stationName+"?").setMessage("The selected host’s listeners will hear this phone’s Player or DJ master, including pads and scratch. Headphone cue stays private. The phone must stay connected while sending.").setPositiveButton("Send phone master",(dialog,index)->{if(selected(selected))report(listener.startPhoneBroadcast(selected));}).setNegativeButton("Cancel",null).show();
        });stationControls.addView(phoneSend);
        phoneStop=action("Stop only this phone’s sender",()->{listener.stopPhoneBroadcast();report("Stopping this phone’s sender; unrelated host programs are not stopped.");});stationControls.addView(phoneStop);
        phoneSendNote=label(listener.phoneBroadcastStatus(),14);stationControls.addView(phoneSendNote);
        capabilityNote=label("Host capabilities have not been checked.",14);stationControls.addView(capabilityNote);
        UiSections.fold(this,colors,"Advanced · station source and diagnostics",getChildCount()-1);
        feedbackForActions=null;
        sharedLists=new LinearLayout(activity);sharedLists.setOrientation(VERTICAL);addView(sharedLists);
        sharedFeedback=label("",14);addView(sharedFeedback);
        addView(action("Offline playlists",this::offline));

        search=new EditText(activity);search.setSingleLine(true);search.setTextColor(colors.text);search.setHintTextColor(colors.muted);search.setHint("Search songs in this station");search.setContentDescription("Search station songs");addView(search);
        queue=label("Find a server to see its music.",14);addView(queue);
        search.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int a,int c,int d){}public void onTextChanged(CharSequence t,int a,int b,int c){renderQueue();}public void afterTextChanged(android.text.Editable t){}});
        addView(action("Help",()->new android.app.AlertDialog.Builder(activity).setTitle("Radio help").setMessage("Station access selects a host without changing playback. Local listening plays its stream here. Server speakers play it on the host audio output. Remote desktop broadcast changes the host source to its desktop Player/DJ master. Google Cast asks a Google speaker to fetch the host stream. Sending this phone’s own playback is not implemented. Internet radio and offline playlists are separate listening choices.").setPositiveButton("Done",null).show()));enabled(false);
        if(listener.playlistSyncAvailable())addView(action("Enable saved-playlist sync",()->{CastClient selected=client;if(selected==null||!verified){Toast.makeText(activity,"Connect to an authorized host first",Toast.LENGTH_LONG).show();return;}listener.playlistSync(selected);}));
        buildDeviceCards();
        renderServers(hostSession.selection.merge(java.util.Collections.emptyList()));
        CastHostSelection.Host saved=hostSession.selection.selected();
        if(saved!=null){address.setText(saved.base);status.setText("Reconnecting to selected host…");now.setText("Your selected server");}
        watchNetwork();
        handler.post(this::resumeConnection);
        handler.postDelayed(poll,3000);
    }
    /** Cards expose the common listening path; existing administrative tools stay intact below. */
    private void buildDeviceCards(){
        java.util.List<android.view.View> original=new java.util.ArrayList<>();for(int i=0;i<getChildCount();i++)original.add(getChildAt(i));
        removeAllViews();setBackgroundColor(android.graphics.Color.TRANSPARENT);setPadding(0,dp(4),0,0);
        LinearLayout program=card(true);stationSummary=label("No station selected",18);stationSummary.setTypeface(null,android.graphics.Typeface.BOLD);stationSummary.setCompoundDrawablesWithIntrinsicBounds(new CardIcon(activity,"server",colors.accent),null,new CardIcon(activity,"spectrum",colors.accent),null);stationSummary.setCompoundDrawablePadding(dp(8));program.addView(stationSummary);stationSummary.setContentDescription("Station · listening & control");
        LinearLayout playing=card(false);TextView heading=label("NOW PLAYING",11);heading.setTextColor(colors.accent);playing.addView(heading);now.setTextSize(16);now.setPadding(0,0,0,dp(4));playing.addView(now);
        stationControls.removeView(listen);listen.setTextColor(android.graphics.Color.BLACK);listen.setBackground(Chrome.button(colors,true));listen.setOnClickListener(v->{if(listener.phoneListening())listener.stopListening();else if(client!=null&&verified&&running)listener.listen(client.phoneStream(),stationName);refreshAvailability();});
        Button destination=action("This phone ▾",()->new android.app.AlertDialog.Builder(activity).setTitle("Choose audio destination").setMessage("Phone listening follows Android's selected speaker, headphones or Bluetooth output. Google Cast receives the host stream directly.").setItems(new String[]{"Phone audio settings","Google Cast host stream"},(dialog,index)->{if(index==0)activity.startActivity(new android.content.Intent(android.provider.Settings.ACTION_SOUND_SETTINGS));else chromecast.performClick();}).setNegativeButton("Close",null).show());
        tiles(playing,listen,destination);
        LinearLayout info=new LinearLayout(activity);info.setOrientation(VERTICAL);addView(info);Button server=action("Server\nNot connected",()->{connectionFields.setVisibility(VISIBLE);reveal(connectionFields);});serverSummary=server;Button output=action("Output\nThis phone · system route",()->activity.startActivity(new android.content.Intent(android.provider.Settings.ACTION_SOUND_SETTINGS)));icon(server,new CardIcon(activity,"server",colors.accent));icon(output,new DevicesIcon(activity,"speaker",colors.accent));tiles(info,server,output);
        addView(section("Stations"));dashboardStations=new LinearLayout(activity);dashboardStations.setOrientation(VERTICAL);addView(dashboardStations);selectedStation=action("Selected host · listen",()->listen.performClick());dashboardStations.addView(selectedStation);dashboardStations.addView(action("Smart Radio · choose playlist  ▶",()->stationPlaylist.performClick()));
        addView(section("Discover"));LinearLayout discovery=new LinearLayout(activity);discovery.setOrientation(VERTICAL);addView(discovery);find.setText("Nearby AMP servers");
        android.view.View radioToggle=null,radioBody=null;for(int i=0;i<original.size();i++)if(android.text.TextUtils.equals("Internet radio · tap a result to listen",original.get(i).getContentDescription())){radioToggle=original.get(i);radioBody=original.get(i+1);break;}
        final android.view.View toggle=radioToggle,body=radioBody;Button internet=action("Internet radio",()->{if(toggle!=null)toggle.performClick();if(body!=null&&body.getVisibility()==VISIBLE)reveal(body);});icon(find,new DevicesIcon(activity,"radio",colors.accent));icon(internet,new DevicesIcon(activity,"globe",colors.accent));tiles(discovery,find,internet);discovery.addView(serverList);if(radioBody!=null)discovery.addView(radioBody);
        addView(section("Quick Actions"));LinearLayout quick=new LinearLayout(activity);quick.setOrientation(VERTICAL);addView(quick);Button offline=action("Offline",this::offline),saved=action("Saved Playlists",()->reveal(libraryView)),remote=action("Remote",listener::remote);icon(offline,new CardIcon(activity,"download",colors.accent));icon(saved,new CardIcon(activity,"playlist",colors.accent));icon(remote,new DevicesIcon(activity,"remote",colors.accent));tiles(quick,offline,saved,remote);
        int advancedStart=getChildCount();LinearLayout advanced=new LinearLayout(activity);advanced.setOrientation(VERTICAL);addView(advanced);for(android.view.View view:original)if(view.getParent()==null&&view!=radioToggle)advanced.addView(view);UiSections.fold(this,colors,"More · library, station controls & diagnostics",advancedStart);
        enabled(client!=null);status.setAccessibilityLiveRegion(android.view.View.ACCESSIBILITY_LIVE_REGION_POLITE);
    }
    private void icon(Button button,android.graphics.drawable.Drawable drawable){button.setCompoundDrawablesWithIntrinsicBounds(drawable,null,null,null);button.setCompoundDrawablePadding(dp(4));}
    private TextView section(String text){TextView title=label(text,17);title.setCompoundDrawablesWithIntrinsicBounds(new DevicesIcon(activity,text.equals("Stations")?"radio":"compass",colors.accent),null,null,null);title.setCompoundDrawablePadding(dp(6));title.setTypeface(null,android.graphics.Typeface.BOLD);title.setPadding(dp(2),dp(4),0,dp(3));return title;}
    private void tiles(LinearLayout parent,Button...buttons){boolean stack=getResources().getConfiguration().fontScale>1.25f||getResources().getConfiguration().screenWidthDp<340;LinearLayout row=new LinearLayout(activity);row.setOrientation(stack?VERTICAL:HORIZONTAL);row.setBaselineAligned(false);parent.addView(row,new LinearLayout.LayoutParams(-1,-2));for(Button button:buttons){button.setMinWidth(0);button.setMinimumWidth(0);button.setTextSize(12);button.setPadding(dp(5),dp(3),dp(5),dp(3));button.setMinHeight(dp(48));LinearLayout.LayoutParams params=new LinearLayout.LayoutParams(stack?-1:0,-2,stack?0:1);params.setMargins(dp(2),dp(2),dp(2),dp(3));row.addView(button,params);}}
    private LinearLayout card(boolean highlight){LinearLayout box=new LinearLayout(activity);box.setOrientation(VERTICAL);box.setPadding(dp(9),dp(5),dp(9),dp(5));box.setBackground(highlight?Chrome.highlighted(colors,colors.accent):Chrome.panel(colors,false));LinearLayout.LayoutParams params=new LinearLayout.LayoutParams(-1,-2);params.setMargins(0,0,0,dp(5));addView(box,params);return box;}
    private void reveal(android.view.View view){android.view.View child=view;while(child!=this&&child.getParent() instanceof android.view.View){child.setVisibility(VISIBLE);child=(android.view.View)child.getParent();}view.post(()->view.requestRectangleOnScreen(new android.graphics.Rect(0,0,view.getWidth(),Math.max(dp(80),view.getHeight())),false));}
    int dp(int n){return (int)(n*getResources().getDisplayMetrics().density);}
    TextView label(String text,int size){TextView v=new TextView(activity);v.setText(text);v.setTextColor(colors.text);v.setTextSize(size);v.setPadding(0,dp(6),0,dp(6));return v;}
    private EditText field(String hint,boolean secret){EditText v=new EditText(activity);v.setHint(hint);v.setSingleLine(true);v.setTextColor(colors.text);v.setHintTextColor(colors.muted);v.setInputType(InputType.TYPE_CLASS_TEXT|(secret?InputType.TYPE_TEXT_VARIATION_PASSWORD:InputType.TYPE_TEXT_VARIATION_URI));connectionFields.addView(v);return v;}
    Button action(String text,Runnable action){Button b=new Button(activity);b.setText(text);b.setAllCaps(false);b.setTextColor(Chrome.ink(colors,false));b.setBackground(Chrome.button(colors,false));b.setMinHeight(dp(48));final TextView feedback=feedbackForActions;b.setOnClickListener(v->{actionFeedback=feedback;action.run();});return b;}
    private TextView path(String title){
        stationControls.addView(label(title,18));TextView note=label("Connect to a host first.",14);stationControls.addView(note);
        feedbackForActions=label("",14);feedbackViews.add(feedbackForActions);stationControls.addView(feedbackForActions);return note;
    }
    private void report(String message){reportTo(message,actionFeedback);}
    private void reportTo(String message,TextView target){status.setText(message);if(target!=null)target.setText(message);}
    void phoneFeedback(String message){feedbackViews.get(0).setText(message);refreshAvailability();}
    boolean selected(CastClient expected){
        if(closed||client!=expected||!verified||busy){report("Host selection or status changed. Refresh and choose this action again.");return false;}return true;
    }
    private void enabled(boolean connected){
        stationControls.setVisibility(VISIBLE);search.setVisibility(connected&&!sharingVisible?VISIBLE:GONE);queue.setVisibility(connected&&!sharingVisible?VISIBLE:GONE);
        boolean ready=connected&&verified&&!busy;
        if(serverSummary!=null)serverSummary.setText(connected?(verified?"Server · connected\n":"Server · reconnecting\n")+stationName:"Server\nNot connected");
        if(stationSummary!=null)stationSummary.setText(stationName+" · "+(!connected?"Not connected":!verified?"Checking connection":preparing?"Preparing":running?"ON AIR":"Stopped"));
        if(!connected)capabilityNote.setText("Connect to an AMP host to enable station, listening, speaker and desktop controls. Connecting does not start playback.");
        else if(!verified)capabilityNote.setText("Host status is unverified. Refresh or reconnect before using host controls. Stop phone playback remains available.");
        boolean control=ready&&(!passwordRequired||client.hasPassword())&&!authRejected;
        if(selectedStation!=null){selectedStation.setText(stationName+" · "+(running?"ON AIR · Listen":"Host not on air"));selectedStation.setEnabled(ready&&running&&listener.phoneAvailable());}
        start.setEnabled(control&&playlistSupported&&!running&&!preparing);stop.setEnabled(control&&(running||preparing));listen.setText(listener.phoneListening()?"■ Stop Listening":"▶ Listen");listen.setTextColor(listener.phoneListening()||ready&&running?android.graphics.Color.BLACK:colors.text);listen.setEnabled(listener.phoneListening()||ready&&running&&listener.phoneAvailable());chromecast.setEnabled(ready&&running);
        phoneSend.setEnabled(control&&listener.phoneSenderAvailable()&&!listener.phoneSending());phoneStop.setEnabled(listener.phoneSending());phoneSendNote.setText(listener.phoneBroadcastStatus());
        hostBroadcast.setEnabled(control&&hostLaunchSupported);serverPlay.setEnabled(control&&serverOutputSupported&&running&&!serverOutputRunning);serverStop.setEnabled(control&&serverOutputSupported&&serverOutputRunning);stationPlaylist.setEnabled(control&&playlistSupported);
        setChildrenEnabled(sharedLists,control);
        String prerequisite=!connected?"Connect to a host first.":!verified?"Refresh or reconnect to verify this host.":preparing?"Host is preparing audio; listening becomes available when on air.":!running?"Host is stopped; start a source first.":"Host stream is on air.";
        localNote.setText(prerequisite);castNote.setText(prerequisite+" The Google speaker must reach the host LAN address.");
        String access=!connected||!verified?"Connect and verify the host first.":!control&&!busy?"Enter the admin password in Advanced connection settings and reconnect.":"";
        sourceNote.setText(access.isEmpty()?(playlistSupported?(preparing?"Preparing · Stop cancels this host program.":running?"Program active · controls affect all listeners.":"Choose a playlist or start the host station."):"Playlist controls are not advertised by this host."):access);
        outputNote.setText(access.isEmpty()?(serverOutputSupported?"Server speakers: "+(serverOutputRunning?"process active; audibility unverified": "off")+". "+prerequisite:"Server speaker controls are not advertised by this host."):access);
        desktopNote.setText(access.isEmpty()?(hostLaunchSupported?"Requests the remote desktop master, not phone audio. Desktop readiness is checked on request.":"Desktop broadcast is not advertised by this host."):access);
        refresh.setEnabled(connected&&!busy);connect.setEnabled(!busy);find.setEnabled(!busy);address.setEnabled(!busy);password.setEnabled(!busy);
        for(Button button:new Button[]{start,stop,listen,refresh,chromecast,hostBroadcast,serverPlay,serverStop,stationPlaylist,connect,find})button.setAlpha(button.isEnabled()?1f:.35f);
    }
    private void setChildrenEnabled(android.view.View view,boolean value){
        view.setEnabled(value);if(view instanceof Button)view.setAlpha(value?1f:.35f);
        if(view instanceof android.view.ViewGroup){android.view.ViewGroup group=(android.view.ViewGroup)view;for(int i=0;i<group.getChildCount();i++)setChildrenEnabled(group.getChildAt(i),value);}
    }
    interface Operation {String run()throws Exception;}
    void work(Operation operation){
        if(closed)return;if(busy){report("Another request is finishing. Try again when it completes.");return;}
        final TextView target=actionFeedback;busy=true;operationVersion++;enabled(client!=null);reportTo("Updating host…",target);
        worker.execute(()->{String message;Exception failure=null;try{message=operation.run();}catch(Exception error){failure=error;message="Selected server request failed. Check access and connection in Advanced.";}
            final String result=message;final Exception error=failure;activity.runOnUiThread(()->{if(closed)return;busy=false;if(error!=null)diagnostics.setText(error.getMessage());
                if(error instanceof CastClient.AuthorizationException){
                    CastClient.AuthorizationException rejection=(CastClient.AuthorizationException)error;
                    authRejected=passwordRequired||rejection.code==401;
                    if(authRejected)connectionFields.setVisibility(VISIBLE);
                } else if(error instanceof CastClient.HostUnavailableException){verified=false;pollFailed=true;recoveryFeedback=target;}
                reportTo(hostError.isEmpty()?result:result+"\nHost reported an error; see Advanced diagnostics.",target);enabled(client!=null);
            });});
    }
    private JSONObject sharedManifest(CastClient selected,JSONObject snapshot)throws Exception {
        if(snapshot.optJSONObject("capabilities")==null||!snapshot.getJSONObject("capabilities").optBoolean("smart_sharing"))return null;
        try{return selected.get("/api/smart/manifest");}catch(CastClient.AuthorizationException error){return snapshot.optBoolean("password_required")||error.code==401?new JSONObject().put("auth_required",true):null;}
    }
    private void connect(){connect(null);}
    private void connect(String expectedIdentity){
        if(busy||closed)return;
        final long selectionGeneration=hostSession.selection.generation();final String url=address.getText().toString(),secret=password.getText().toString();client=null;verified=false;running=false;preparing=false;passwordRequired=false;authRejected=false;sharingVisible=false;pollFailed=false;recoveryFeedback=null;sharedFeedback.setText("");hostError="";sharedFingerprint="";serverOutputRunning=false;stationName="AMP Cast";tracks=new JSONArray();sharedLists.removeAllViews();now.setText("No station selected");for(TextView feedback:feedbackViews)feedback.setText("");hostLaunchSupported=false;serverOutputSupported=false;playlistSupported=false;enabled(false);
        work(()->{CastClient initial=new CastClient(url,secret);JSONObject first=initial.connect();verifyIdentity(first,expectedIdentity);String restored=secret;
            if(restored.isEmpty())try{restored=ModuleRpc.call(activity,"playlist_sync_access",new JSONObject().put("base",initial.base).put("server_id",first.getString("server_id")).put("library_id",first.getString("library_id"))).getString("password");}catch(Exception unavailable){}
            final CastClient candidate=restored.equals(secret)?initial:new CastClient(url,restored);final JSONObject snapshot=candidate==initial?first:candidate.connect();verifyIdentity(snapshot,expectedIdentity);JSONObject list=candidate.playlist(),manifest=sharedManifest(candidate,snapshot);
            activity.runOnUiThread(()->{if(closed||!hostSession.selection.current(selectionGeneration))return;client=candidate;connectionFields.setVisibility(GONE);hostSession.remember(candidate,snapshot.optString("server_id"),snapshot.optString("station_id"));renderServers(hostSession.selection.merge(java.util.Collections.emptyList()));render(snapshot,list);renderShared(manifest);libraryView.bind(candidate,snapshot);});return "Host connected. Public listening is independent of admin access; no playback was started.";});
    }
    private void readStatus(CastClient selected)throws Exception {
        JSONObject snapshot=selected.status(),list=selected.playlist(),manifest=sharedManifest(selected,snapshot);
        activity.runOnUiThread(()->{if(!closed&&client==selected){render(snapshot,list);renderShared(manifest);}});
    }
    private void postAndRefresh(CastClient selected,String path,JSONObject body)throws Exception {selected.post(path,body);readStatus(selected);}
    void refresh(){refresh(true);}
    private void refresh(boolean foreground){
        final CastClient selected=client;if(selected==null)return;
        if(foreground){work(()->{readStatus(selected);return "Host status refreshed.";});return;}
        if(polling||busy||closed)return;polling=true;final long version=operationVersion;
        worker.execute(()->{
            JSONObject snapshot=null,list=null,manifest=null;Exception failure=null;
            try{snapshot=selected.status();list=selected.playlist();manifest=sharedManifest(selected,snapshot);}catch(Exception e){failure=e;}
            final JSONObject state=snapshot,tracks=list,shared=manifest;final Exception error=failure;
            activity.runOnUiThread(()->{polling=false;if(closed||client!=selected||version!=operationVersion)return;
                if(error!=null){verified=false;pollFailed=true;hostSession.selection.failed(android.os.SystemClock.elapsedRealtime());reportTo("Host status unavailable: "+error.getMessage(),null);}
                else {boolean recovered=!verified||pollFailed;pollFailed=false;render(state,tracks);renderShared(shared);if(recovered){reportTo("Host connection recovered. Status is current; no command was repeated."+(hostError.isEmpty()?"":"\nHost program: "+hostError),recoveryFeedback);recoveryFeedback=null;}}
                enabled(client!=null);
            });
        });
    }
    private void command(boolean launch){final CastClient selected=client;if(selected==null||!verified||(launch&&!playlistSupported))return;work(()->{selected.command(launch);JSONObject snapshot=selected.status(),list=selected.playlist();activity.runOnUiThread(()->{if(!closed&&client==selected)render(snapshot,list);});return launch?"Start requested; see current host state.":"Stop requested; see current host state.";});}
    private void render(JSONObject snapshot,JSONObject list){
        verified=true;pollFailed=false;hostSession.selection.recovered();
        JSONObject capabilities=snapshot.optJSONObject("capabilities");
        hostLaunchSupported=capabilities!=null&&capabilities.optBoolean("host_master_launch",false);
        serverOutputSupported=capabilities!=null&&capabilities.optBoolean("server_output_control",false);
        playlistSupported=capabilities!=null&&capabilities.optBoolean("playlist_playback",false);
        boolean outputWasRunning=serverOutputRunning;
        serverOutputRunning=snapshot.optBoolean("server_output",false);
        if(outputWasRunning&&!serverOutputRunning)reportTo("Server speaker output is now off. If this was unexpected, check FFplay and the host audio device.",feedbackViews.get(2));
        passwordRequired=snapshot.optBoolean("password_required");
        String nextError=snapshot.isNull("error")?"":snapshot.optString("error","");
        if(!nextError.equals(hostError)){String message=nextError.isEmpty()?"Host program error cleared; see current source status.":"Host reported an error; see Advanced diagnostics.";reportTo(message,feedbackViews.get(3));feedbackViews.get(4).setText(message);}
        hostError=nextError;if(!nextError.isEmpty())diagnostics.setText(nextError);
        capabilityNote.setText((snapshot.optBoolean("preparing")?"Host is preparing audio":snapshot.optBoolean("on_air",snapshot.optBoolean("running"))?"Host stream is on air":"Host is stopped")+"\nStation source: "+(playlistSupported?"playlist controls available":"playlist controls not advertised by this host")+"\nDesktop host broadcast: "+(hostLaunchSupported?"command supported; desktop readiness checked when requested":"not advertised; update/check selected host")+"\nServer speakers: "+(serverOutputSupported?"command supported; physical output unverified":"not advertised; update/check selected host"));
        running=snapshot.optBoolean("on_air",snapshot.optBoolean("running"));preparing=snapshot.optBoolean("preparing");
        stationName=snapshot.optString("station_name","AMP Cast");
        now.setText(snapshot.isNull("now_playing")?(running?"Live station broadcast":"Nothing playing · choose a station"):snapshot.optString("now_playing"));
        now.setContentDescription(now.getText()+". Source: "+snapshot.optString("source","unknown")+". Listeners: "+snapshot.optInt("listeners"));

        tracks=list.optJSONArray("tracks");if(tracks==null)tracks=new JSONArray();renderQueue();
    }
    private void renderShared(JSONObject manifest){
        TextView previousFeedback=feedbackForActions;feedbackForActions=sharedFeedback;try{
        String fingerprint=manifest==null?"off":manifest.toString();
        if(fingerprint.equals(sharedFingerprint))return;sharedFingerprint=fingerprint;
        sharedLists.removeAllViews();sharingVisible=manifest!=null&&!manifest.optBoolean("auth_required");
        if(manifest!=null&&manifest.optBoolean("auth_required")){authRejected=true;connectionFields.setVisibility(VISIBLE);sharedLists.addView(label("Enter the admin password and reconnect for playlist downloads. Public listening remains available.",14));return;}
        if(manifest!=null)authRejected=false;
        if(manifest==null){if(client!=null){sharedLists.addView(label("Playlist downloads are off on this host. Live listening does not save songs or cues.",14));final CastClient owner=client;sharedLists.addView(action("Enable playlist downloads…",()->new android.app.AlertDialog.Builder(activity).setTitle("Share this host's playlists?").setMessage("Allow connected devices with host access to download playlist audio and prepared cues. This does not change the live DJ broadcast.").setPositiveButton("Enable",(d,n)->{if(!selected(owner))return;final CastClient selected=owner;work(()->{selected.post("/api/smart/enable",null);JSONObject available=selected.get("/api/smart/manifest");activity.runOnUiThread(()->{if(!closed&&client==selected)renderShared(available);});return "Choose a playlist below, then Save offline with my cues.";});}).setNegativeButton("Cancel",null).show()));}return;}queue.setVisibility(GONE);sharedLists.addView(label("Station playlists",18));JSONArray lists=manifest.optJSONArray("playlists");if(lists==null)return;
        for(int i=0;i<lists.length();i++){JSONObject playlist=lists.optJSONObject(i);if(playlist==null)continue;sharedLists.addView(label(playlist.optString("name")+" · "+playlist.optJSONArray("tracks").length()+" songs",16));final CastClient selected=client;sharedLists.addView(action("Start host playlist",()->{if(!selected(selected))return;work(()->{selected.playPlaylist(playlist.optString("id"));readStatus(selected);return "Host playlist requested. When on air, choose Listen on this phone.";});}));sharedLists.addView(action("Save offline with my cues",()->downloadPlaylist(selected,manifest,playlist)));}
        }finally{feedbackForActions=previousFeedback;}
    }
    private void downloadSaved(String id){final CastClient owner=client;if(!selected(owner))return;final JSONObject[] found=new JSONObject[2];work(()->{owner.post("/api/smart/enable",null);JSONObject manifest=owner.get("/api/smart/manifest");JSONArray lists=manifest.getJSONArray("playlists");for(int i=0;i<lists.length();i++)if(id.equals(lists.getJSONObject(i).optString("id")))found[1]=lists.getJSONObject(i);if(found[1]==null)throw new java.io.IOException("Enable sharing for this saved playlist on the server before downloading. Streaming remains separate.");found[0]=manifest;activity.runOnUiThread(()->finishDownloadSelection(owner,found));return "Download selected saved playlist";});}
    private void finishDownloadSelection(CastClient owner,JSONObject[] found){if(closed||client!=owner)return;if(busy){handler.postDelayed(()->finishDownloadSelection(owner,found),30);return;}downloadPlaylist(owner,found[0],found[1]);}
    void downloadPlaylist(CastClient selected,JSONObject manifest,JSONObject playlist){}
    private void chooseStationPlaylist(){final CastClient selected=client;if(selected==null){status.setText("Connect to a station first");return;}work(()->{JSONArray lists=selected.get("/api/station/playlists").getJSONArray("playlists");if(lists.length()==0)return "This host has no saved station playlists. Create one on the host first.";activity.runOnUiThread(()->{if(closed||client!=selected)return;String[] names=new String[lists.length()];for(int i=0;i<names.length;i++)names[i]=lists.optJSONObject(i).optString("name");new android.app.AlertDialog.Builder(activity).setTitle("Launch independent Smart Radio").setItems(names,(d,i)->{if(!selected(selected))return;work(()->{postAndRefresh(selected,"/api/station/play/"+android.net.Uri.encode(lists.optJSONObject(i).optString("id")),null);return "Smart Radio requested; see current host state.";});}).setNegativeButton("Cancel",null).show();});return "Choose a saved playlist";});}
    void offline(){}
    void refreshAvailability(){enabled(client!=null);}
    private void renderQueue(){
        String query=search.getText().toString().trim().toLowerCase(java.util.Locale.ROOT);StringBuilder text=new StringBuilder();int total=0,shown=0;
        for(int i=0;i<tracks.length();i++){JSONObject track=tracks.optJSONObject(i);if(track==null)continue;String title=track.optString("title","Track");if(!title.toLowerCase(java.util.Locale.ROOT).contains(query))continue;total++;if(shown++<200)text.append(i+1).append(". ").append(title).append('\n');}
        if(total>200)text.append("Showing 200 of ").append(total).append(" matches. Refine your search.");
        queue.setText(text.length()==0?(client==null?"Find a server to see its music.":query.isEmpty()?"No songs in this station yet.":"No songs match your search."):text.toString());
    }
    private static void verifyIdentity(JSONObject snapshot,String expected)throws java.io.IOException {
        CastHostReconnect.verify(snapshot,expected);
    }
    /** A screen can be replaced at any time. Reverify, never replay a command or auto-listen. */
    void resumeConnection(){
        if(closed||busy||polling)return;
        final CastHostSelection.Host saved=hostSession.selection.selected();
        if(saved==null||!hostSession.selection.retryDue(android.os.SystemClock.elapsedRealtime()))return;
        if(client!=null&&verified){refresh(false);return;}
        polling=true;verified=false;enabled(client!=null);
        final long generation=hostSession.selection.generation(),version=operationVersion;
        worker.execute(()->{
            CastClient candidate=null;JSONObject snapshot=null,list=null,manifest=null;Exception failure=null;
            try{
                String base=saved.base;
                if(hostSession.selection.discoveryDue()){
                    java.util.List<CastDiscovery.Server> fresh=CastDiscovery.find();
                    final java.util.List<CastDiscovery.Server> known=hostSession.selection.merge(fresh);
                    activity.runOnUiThread(()->{if(!closed&&hostSession.selection.current(generation))renderServers(known);});
                    for(CastDiscovery.Server server:fresh)if(saved.identity().equals(server.id)){base=server.url;break;}
                }
                CastHostReconnect.Verified restored=CastHostReconnect.restore(base,saved.identity(),hostSession.client());
                candidate=restored.client;snapshot=restored.status;
                list=candidate.playlist();manifest=sharedManifest(candidate,snapshot);
            }catch(Exception error){failure=error;}
            final CastClient restored=candidate;final JSONObject state=snapshot,items=list,shared=manifest;final Exception error=failure;
            activity.runOnUiThread(()->{
                polling=false;
                if(closed||version!=operationVersion||!hostSession.selection.current(generation))return;
                if(error!=null){
                    verified=false;pollFailed=true;hostSession.selection.failed(android.os.SystemClock.elapsedRealtime());
                    status.setText("Selected host retained · reconnecting automatically. "+error.getMessage());
                    now.setText("Your selected server is temporarily unavailable");
                }else{
                    client=restored;address.setText(restored.base);
                    hostSession.remember(restored,state.optString("server_id"),state.optString("station_id"));
                    render(state,items);renderShared(shared);libraryView.bind(restored,state);
                    status.setText("Selected host reconnected. No playback was started or repeated.");
                }
                enabled(client!=null);
            });
        });
    }
    private void disconnectHost(){
        operationVersion++;hostSession.disconnect();client=null;verified=false;running=false;preparing=false;
        password.setText("");address.setText("");sharedLists.removeAllViews();sharedFingerprint="";sharingVisible=false;
        tracks=new JSONArray();renderQueue();libraryView.setVisibility(GONE);renderServers(hostSession.selection.merge(java.util.Collections.emptyList()));now.setText("No station selected");enabled(false);
        status.setText("Host selection cleared. Existing phone playback and server playback were not changed.");
    }
    private void watchNetwork(){
        try{
            connectivity=(android.net.ConnectivityManager)activity.getSystemService(Context.CONNECTIVITY_SERVICE);
            networkCallback=new android.net.ConnectivityManager.NetworkCallback(){
                private void changed(){handler.post(()->{
                    if(closed)return;
                    android.net.Network active=connectivity.getActiveNetwork();
                    String signature=active==null?"offline":active.toString();
                    if(hostSession.selection.networkChanged(signature)){verified=false;resumeConnection();}
                });}
                @Override public void onAvailable(android.net.Network network){changed();}
                @Override public void onLost(android.net.Network network){changed();}
                @Override public void onCapabilitiesChanged(android.net.Network network,android.net.NetworkCapabilities capabilities){changed();}
            };
            connectivity.registerDefaultNetworkCallback(networkCallback);
        }catch(RuntimeException unavailable){networkCallback=null;}
    }
    private void renderServers(java.util.List<CastDiscovery.Server> servers){
        serverList.removeAllViews();
        CastHostSelection.Host saved=hostSession.selection.selected();
        if(saved!=null)serverList.addView(label("Your selected server · remembered across navigation",14));
        java.util.Map<String,Integer> counts=new java.util.HashMap<>(),shown=new java.util.HashMap<>();
        for(CastDiscovery.Server server:servers)counts.merge(server.name,1,Integer::sum);
        for(CastDiscovery.Server server:servers){
            String name=server.name.trim().isEmpty()?"AMP Verse Server":server.name;
            if(counts.get(server.name)>1)name+=" ("+shown.merge(server.name,1,Integer::sum)+")";
            serverList.addView(action(name,()->{address.setText(server.url);password.setText("");connect(server.id);}));
        }
    }
    void findServers() {
        work(()->{
            java.util.List<CastDiscovery.Server> fresh=CastDiscovery.find();
            final java.util.List<CastDiscovery.Server> known=hostSession.selection.merge(fresh);
            activity.runOnUiThread(()->{if(!closed)renderServers(known);});
            return fresh.isEmpty()
                ?"No servers replied this time. Last known hosts and your selected host are retained; check the network or retry."
                :"Choose a server to connect. Previously selected host is unchanged.";
        });
    }
    void close(){if(connectivity!=null&&networkCallback!=null)try{connectivity.unregisterNetworkCallback(networkCallback);}catch(RuntimeException ignored){}if(libraryView!=null)libraryView.close();closed=true;handler.removeCallbacksAndMessages(null);worker.shutdownNow();}
}
