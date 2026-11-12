package cloud.gitwide.ampverse;
import android.app.*;
import android.widget.*;
import org.json.*;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;

/** Explicit desktop control through the existing Companion API. */
final class RemotePanel extends LinearLayout {
    private final Activity activity;private final Appearance colors;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final ExecutorService discoveryWorkers=Executors.newFixedThreadPool(2);
    private final Button discoverButton;
    private final TextView discoveryStatus;
    private final LinearLayout nearbyDevices;
    private int discoveryGeneration,discoveryPending,discoveryFound,discoveryErrors;
    private final EditText address,password,search;private final TextView status,now;
    private final LinearLayout songs,controls;
    private JSONObject latestState=new JSONObject();
    private JSONArray catalog=new JSONArray(),playlists=new JSONArray();
    private String base="",secret="",session="",deck="a";private boolean closed,connected;
    private static String rememberedBase="",rememberedSecret="";
    private int generation;private final android.os.Handler handler=new android.os.Handler(android.os.Looper.getMainLooper());
    private final okhttp3.OkHttpClient socketClient=new okhttp3.OkHttpClient.Builder().pingInterval(10,TimeUnit.SECONDS).build();
    private okhttp3.WebSocket socket;
    RemotePanel(Activity a,Appearance c){super(a);activity=a;colors=c;setOrientation(VERTICAL);setPadding(12,12,12,12);
        addView(label("Control desktop AMP Verse",22));
        discoverButton=button("Discover devices",this::discover);discoverButton.setContentDescription("Discover AMP Verse devices");addView(discoverButton);
        discoveryStatus=label("Find Player, DJ and Cast hosts on your Wi-Fi.",14);addView(discoveryStatus);
        nearbyDevices=new LinearLayout(a);nearbyDevices.setOrientation(VERTICAL);addView(nearbyDevices);
        address=field("http://192.168.1.50:8790",false);address.setText(a.getSharedPreferences("settings",0).getString("remote-server",""));address.setContentDescription("Desktop server address");password=field("Optional control password",true);password.setContentDescription("Desktop control password");
        if(address.getText().toString().equals(rememberedBase))password.setText(rememberedSecret);
        addView(button("Connect / Refresh",this::connect));
        addView(button("Disconnect remote",this::disconnect));status=label("Choose the desktop control server. Cast radio has its own screen.",14);addView(status);now=label("Not connected",18);addView(now);
        controls=new LinearLayout(a);controls.setOrientation(VERTICAL);addView(controls);controls.setVisibility(GONE);
        row(controls,button("A",()->{deck="a";status.setText("Selected deck A");}),button("B",()->{deck="b";status.setText("Selected deck B");}));
        row(controls,button("Play",()->command("play","deck",deck)),button("Pause",()->command("pause","deck",deck)),button("Stop",()->command("stop","deck",deck)));
        row(controls,button("Restart cue",()->command("restart_cue","deck",deck)),button("Preload next",()->command("preload_next")),button("Mix next",()->command("mix_next","immediate",true)));
        row(controls,button("Auto DJ ON",()->command("auto_dj","enabled",true)),button("Auto DJ OFF",()->command("auto_dj","enabled",false)));
        row(controls,button("Crossfade ON",()->command("auto_crossfade","enabled",true)),button("Crossfade OFF",()->command("auto_crossfade","enabled",false)));
        row(controls,button("DJ Lock ON",()->command("manual_lock","enabled",true)),button("DJ Lock OFF",()->command("manual_lock","enabled",false)));
        controls.addView(label("Crossfader · A ↔ B",15));SeekBar fader=new SeekBar(a);fader.setMax(100);fader.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){}public void onStopTrackingTouch(SeekBar b){command("set_crossfader","value",b.getProgress()/100.);}public void onProgressChanged(SeekBar b,int value,boolean user){}});controls.addView(fader);
        row(controls,button("Hype",()->command("set_style","style","hype")),button("Steady",()->command("set_style","style","steady")),button("Chill",()->command("set_style","style","chill")));
        controls.addView(label("Target energy",15));SeekBar energy=new SeekBar(a);energy.setMax(100);energy.setProgress(50);energy.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){}public void onStopTrackingTouch(SeekBar b){command("set_energy","target",1.+9.*b.getProgress()/100.);}public void onProgressChanged(SeekBar b,int value,boolean user){}});controls.addView(energy);
        controls.addView(button("Desktop Smart Playlists",this::choosePlaylist));
        controls.addView(button("Deck tools · sync, stems and loops",this::deckTools));
        controls.addView(button("Google speakers on desktop",this::googleSpeakers));
        row(controls,button("Master −",()->command("adjust_volume","delta",-0.05)),button("Master +",()->command("adjust_volume","delta",0.05)));
        row(controls,button("Seek −10s",()->command("seek_relative","deck",deck,"seconds",-10)),button("Seek +10s",()->command("seek_relative","deck",deck,"seconds",10)));
        row(controls,button("Previous",()->command("previous")),button("Next",()->command("player_next")));
        row(controls,button("Shuffle ON",()->command("player_options","shuffle",true)),button("Shuffle OFF",()->command("player_options","shuffle",false)));
        row(controls,button("Repeat ON",()->command("player_options","repeat",true)),button("Repeat OFF",()->command("player_options","repeat",false)));
        controls.addView(button("Progressive energy…",()->{final EditText value=new EditText(activity);value.setHint("Target energy 1–10");value.setInputType(8194);new AlertDialog.Builder(activity).setTitle("Reach target over 4 transitions").setView(value).setPositiveButton("Apply",(d,n)->{try{command("progression","target",Double.parseDouble(value.getText().toString()),"transitions",4,"hold",true);}catch(Exception e){status.setText("Enter energy from 1 to 10");}}).setNegativeButton("Cancel",null).show();}));
        controls.addView(button("Desktop transition techniques",()->{boolean[] enabled={true,true,true,true,true,true};new AlertDialog.Builder(activity).setTitle("Occasional Auto DJ techniques").setMultiChoiceItems(new String[]{"Scratch","Stutter","Tape brake","Filter sweep","Bass swap","Beatmatch"},enabled,(dialog,n,value)->enabled[n]=value).setPositiveButton("Apply",(dialog,n)->{JSONArray flags=new JSONArray();for(boolean value:enabled)flags.put(value);command("transition_settings","enabled",true,"techniques",flags,"every",4);}).setNeutralButton("All effects off",(dialog,n)->command("transition_settings","enabled",false,"techniques",new JSONArray().put(false).put(false).put(false).put(false).put(false).put(false),"every",4)).setNegativeButton("Cancel",null).show();}));
        for(int side=0;side<2;side++){final int s=side;controls.addView(label((s==0?"Left":"Right")+" desktop Cue Deck",17));row(controls,button("Play cue",()->cue(s,"play")),button("Pause cue",()->cue(s,"pause")),button("Stop cue",()->cue(s,"stop")));row(controls,button("Send → A",()->cue(s,"send","deck","a")),button("Send → B",()->cue(s,"send","deck","b")));controls.addView(label((s==0?"Left":"Right")+" desktop sampler",15));for(int r=0;r<3;r++){Button[] pads=new Button[3];for(int col=0;col<3;col++){final int pad=r*3+col+1;pads[col]=button("Pad "+pad,()->command("side_sampler","side",s,"pad",pad));}row(controls,pads);}}
        search=field("Search desktop songs",false);search.setContentDescription("Search desktop music");songs=new LinearLayout(a);songs.setOrientation(VERTICAL);addView(songs);
        search.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int s,int count,int after){}public void onTextChanged(CharSequence t,int s,int before,int count){renderSongs();}public void afterTextChanged(android.text.Editable e){}});
        if(address.getText().toString().equals(rememberedBase)&&!rememberedBase.isEmpty())handler.post(this::connect);
        handler.post(this::discover);
        addView(label("Desktop audio stays on desktop. Use AMP Cast to listen on this phone. This control API does not transfer music, edit NAS metadata or run analysis on Android.",13));
    }
    private TextView label(String text,int size){TextView v=new TextView(activity);v.setText(text);v.setTextSize(size);v.setTextColor(colors.text);v.setPadding(6,10,6,10);return v;}
    private EditText field(String hint,boolean secret){EditText v=new EditText(activity);v.setSingleLine(true);v.setHint(hint);v.setTextColor(colors.text);v.setHintTextColor(colors.muted);v.setInputType(android.text.InputType.TYPE_CLASS_TEXT|(secret?android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD:android.text.InputType.TYPE_TEXT_VARIATION_URI));addView(v);return v;}
    private Button button(String text,Runnable action){Button b=new Button(activity);b.setText(text);b.setAllCaps(false);b.setMinWidth(0);b.setMinimumWidth(0);b.setTextSize(13);b.setPadding(4,8,4,8);b.setTextColor(text.equals("A")||text.endsWith("→ A")?DjEngine.AMBER:text.equals("B")||text.endsWith("→ B")?DjEngine.CYAN:colors.classic?android.graphics.Color.BLACK:colors.text);b.setBackground(Chrome.button(colors,false));b.setOnClickListener(v->action.run());return b;}
    private void row(LinearLayout target,Button...buttons){LinearLayout row=new LinearLayout(activity);for(Button b:buttons)row.addView(b,new LayoutParams(0,(int)(50*getResources().getDisplayMetrics().density),1));target.addView(row);}
    private interface Work {JSONObject run()throws Exception;}
    private void submit(Work work,java.util.function.Consumer<JSONObject> done){if(closed)return;final int expected=generation;worker.execute(()->{try{JSONObject value=work.run();activity.runOnUiThread(()->{if(!closed&&expected==generation)done.accept(value);});}catch(Exception error){activity.runOnUiThread(()->{if(!closed&&expected==generation){status.setText("Desktop: "+describe(error));}});}});}
    private void disconnect(){generation++;connected=false;session="";rememberedBase="";rememberedSecret="";if(socket!=null)socket.cancel();controls.setVisibility(GONE);now.setText("Disconnected · host playback continues");}
    private void discover(){
        if(closed||discoveryPending>0)return;
        final int scan=++discoveryGeneration;
        discoveryPending=2;discoveryFound=0;discoveryErrors=0;
        nearbyDevices.removeAllViews();discoverButton.setEnabled(false);discoverButton.setText("Discovering…");
        discoveryStatus.setText("Searching this Wi-Fi for Player, DJ and Cast hosts…");
        discoveryWorkers.execute(()->{
            try{List<ControlDiscovery.Server> hosts=ControlDiscovery.find();activity.runOnUiThread(()->{
                if(closed||scan!=discoveryGeneration)return;
                for(ControlDiscovery.Server host:hosts){
                    nearbyDevices.addView(button(host.name+" · Player / DJ\n"+host.url.replace("http://",""),()->{
                        if(closed)return;
                        if(!address.getText().toString().equals(host.url))password.setText("");
                        address.setText(host.url);
                        if(host.passwordRequired){status.setText("Device selected. Enter its control password, then Connect.");password.requestFocus();}
                        else connect();
                    }));discoveryFound++;
                }
                discoveryFinished(scan,false);
            });}catch(Exception error){activity.runOnUiThread(()->discoveryFinished(scan,true));}
        });
        discoveryWorkers.execute(()->{
            try{List<CastDiscovery.Server> hosts=CastDiscovery.find();activity.runOnUiThread(()->{
                if(closed||scan!=discoveryGeneration)return;
                for(CastDiscovery.Server host:hosts){
                    nearbyDevices.addView(button(host.name+" · Cast station\n"+host.url.replace("http://",""),()->{
                        if(closed)return;
                        activity.getSharedPreferences("settings",0).edit().putString("cast-server",host.url).apply();
                        activity.startActivity(new android.content.Intent(activity,MainActivity.class).putExtra("module","AMP Cast").addFlags(android.content.Intent.FLAG_ACTIVITY_SINGLE_TOP));
                    }));discoveryFound++;
                }
                discoveryFinished(scan,false);
            });}catch(Exception error){activity.runOnUiThread(()->discoveryFinished(scan,true));}
        });
    }
    private void discoveryFinished(int scan,boolean failed){
        if(closed||scan!=discoveryGeneration)return;
        if(failed)discoveryErrors++;
        discoveryPending--;
        if(discoveryPending==0){discoverButton.setEnabled(true);discoverButton.setText("Discover devices");}
        discoveryStatus.setText(discoveryFound>0
            ?discoveryFound+" nearby device"+(discoveryFound==1?"":"s")+" · tap to connect"+(discoveryPending>0?" · still searching…":discoveryErrors>0?" · some discovery unavailable":"")
            :discoveryPending>0?"Still searching this Wi-Fi…"
            :discoveryErrors>0?"Discovery unavailable. Check Wi-Fi, or enter the host address below."
            :"No hosts found. Use the same Wi-Fi, open desktop AMP Verse and enable Remote control in Library > Sources. You can also enter an address below.");
    }
    private static String describe(Throwable error){if(error instanceof java.net.ConnectException)return "Connection refused. Check desktop control is running on port 8790.";if(error instanceof java.net.UnknownHostException)return "Device unavailable: hostname not found.";if(error instanceof java.net.SocketTimeoutException)return "Device unavailable or connection lost (timeout).";return error.getMessage()==null?error.toString():error.getMessage();}
    private void connect(){try{
        generation++;connected=false;if(socket!=null){socket.cancel();socket=null;}
        CastClient validated=new CastClient(address.getText().toString(),"");base=validated.base;URI hostUri=new URI(base);if(hostUri.getPort()==-1)base=new URI(hostUri.getScheme(),null,hostUri.getHost(),8790,null,null,null).toString();secret=password.getText().toString();session="";
        String endpoint=base,credential=secret;controls.setVisibility(GONE);status.setText("Checking AMP API and password…");
        submit(()->{JSONObject info=request(endpoint,credential,"/api/v1/info",null);if(info.optInt("protocol")!=1||!info.optString("service").equals("amp-control")||info.optString("session").isEmpty())throw new IOException("Incompatible version. Update the desktop AMP Verse host.");JSONObject state=request(endpoint,credential,"/api/v1/state",null);if(!info.optString("session").equals(state.optJSONObject("control").optString("session")))throw new IOException("Host restarted. Connect again.");JSONObject result=request(endpoint,credential,"/api/v1/catalog",null);result.put("state",state).put("control",info);return result;},value->{
            session=value.optJSONObject("control").optString("session");connected=true;rememberedBase=endpoint;rememberedSecret=credential;
            catalog=value.optJSONArray("songs");if(catalog==null)catalog=new JSONArray();playlists=value.optJSONArray("playlists");if(playlists==null)playlists=new JSONArray();JSONArray regular=value.optJSONArray("regular_playlists");if(regular!=null)for(int i=0;i<regular.length();i++){JSONObject p=regular.optJSONObject(i);if(p!=null)playlists.put(p);}
            activity.getSharedPreferences("settings",0).edit().putString("remote-server",endpoint).apply();controls.setVisibility(VISIBLE);status.setText("Connected · authenticated AMP host · "+catalog.length()+" songs");showState(value.optJSONObject("state"));renderSongs();openEvents(generation,endpoint,credential,session);
        });
    }catch(Exception error){status.setText(describe(error));}}
    private void openEvents(final int expected,String endpoint,String credential,String identity){
        okhttp3.Request request=new okhttp3.Request.Builder().url(endpoint.replaceFirst("^http","ws")+"/api/v1/events").header("x-amp-password",credential).header("x-amp-session",identity).build();
        socket=socketClient.newWebSocket(request,new okhttp3.WebSocketListener(){
            @Override public void onMessage(okhttp3.WebSocket ws,String text){if(text.length()>1024*1024){ws.cancel();return;}try{JSONObject state=new JSONObject(text);if(!identity.equals(state.optJSONObject("control").optString("session")))throw new IOException("Host session changed");activity.runOnUiThread(()->{if(!closed&&expected==generation){showState(state);status.setText("Connected · live state updates");}});}catch(Exception e){ws.cancel();}}
            @Override public void onFailure(okhttp3.WebSocket ws,Throwable error,okhttp3.Response response){activity.runOnUiThread(()->lost(expected,error));}
            @Override public void onClosed(okhttp3.WebSocket ws,int code,String reason){activity.runOnUiThread(()->lost(expected,new IOException("Connection lost")));}
        });
    }
    private void lost(int expected,Throwable error){if(closed||expected!=generation)return;connected=false;controls.setVisibility(GONE);status.setText("Connection lost · "+describe(error)+" · reconnecting (commands are not replayed)");handler.postDelayed(()->{if(!closed&&expected==generation)connect();},3000);}
    private void showState(JSONObject state){
        if(state!=null)latestState=state;
        JSONObject amp=state==null?null:state.optJSONObject("amp");if(amp==null){now.setText("Status unavailable");return;}
        StringBuilder text=new StringBuilder(amp.isNull("current_song")?"No track":amp.optString("current_song"));
        text.append(" · ").append(amp.optString("playback_state","stopped")).append(" · ").append(amp.optBoolean("auto_dj_enabled")?"Auto DJ ON":"Auto DJ OFF").append(" · ").append(amp.optBoolean("manual_dj_locked")?"DJ locked":"DJ unlocked");
        text.append("\nPlaylist: ").append(amp.isNull("playlist")?"None":amp.optString("playlist")).append(" · Master ").append(Math.round(amp.optDouble("master_level",0)*100)).append('%');
        JSONArray decks=state.optJSONArray("decks");if(decks!=null)for(int i=0;i<decks.length();i++){JSONObject d=decks.optJSONObject(i);if(d==null)continue;text.append("\n").append(d.optString("deck")).append(": ").append(d.optBoolean("loaded")?d.optString("title"):"Empty").append(" · ").append(d.optString("state"));}
        now.setText(text);
    }
    private void deckTools(){
        LinearLayout panel=new LinearLayout(activity);panel.setOrientation(VERTICAL);panel.setPadding(20,12,20,12);
        panel.addView(label("Selected LIVE deck "+deck.toUpperCase(Locale.ROOT),18));
        row(panel,button("Sync ON",()->command("deck_sync","deck",deck,"enabled",true)),button("Sync OFF",()->command("deck_sync","deck",deck,"enabled",false)));
        row(panel,button("Key Lock ON",()->command("deck_key_lock","deck",deck,"enabled",true)),button("Key Lock OFF",()->command("deck_key_lock","deck",deck,"enabled",false)));
        row(panel,button("Launch on grid",()->command("quantized_launch","deck",deck)),button("4-beat loop",()->command("deck_loop","deck",deck,"enabled",true,"beats",4)),button("Loop OFF",()->command("deck_loop","deck",deck,"enabled",false,"beats",4)));
        panel.addView(label("Prepared stems · processing stays on desktop",14));
        double[] levels={1,1,1};String[] names={"Vocals","Instruments","Beats"};
        JSONArray selected=latestState.optJSONArray("decks");if(selected!=null)for(int i=0;i<selected.length();i++){JSONObject d=selected.optJSONObject(i);if(d!=null&&d.optString("deck").equalsIgnoreCase(deck)){JSONArray saved=d.optJSONArray("stem_levels");if(saved!=null)for(int j=0;j<3;j++)levels[j]=saved.optDouble(j,1);}}
        for(int i=0;i<3;i++){final int band=i;panel.addView(label(names[i],14));SeekBar slider=new SeekBar(activity);slider.setMax(100);slider.setProgress((int)(levels[i]*100));slider.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){}public void onProgressChanged(SeekBar b,int value,boolean user){if(user)levels[band]=value/100.;}public void onStopTrackingTouch(SeekBar b){JSONArray values=new JSONArray();for(double level:levels)values.put((Object)Double.valueOf(level));command("deck_stems","deck",deck,"levels",values);}});panel.addView(slider);}
        new AlertDialog.Builder(activity).setTitle("Desktop deck controls").setView(panel).setPositiveButton("Done",null).show();
    }
    private void googleCommand(String operation,String receiver,String address){try{JSONObject value=new JSONObject().put("operation",operation);if(receiver!=null)value.put("receiver",receiver);if(address!=null)value.put("address",address);command("google_cast","operation",value);}catch(JSONException e){status.setText(e.getMessage());}}
    private void googleSpeakers(){
        JSONObject cast=latestState.optJSONObject("google_cast");JSONArray receivers=cast==null?null:cast.optJSONArray("receivers");
        ArrayList<String> names=new ArrayList<>();names.add("Search for speakers");names.add("Stop casting");names.add("Refresh this list");if(receivers!=null)for(int i=0;i<receivers.length();i++)names.add(receivers.optJSONObject(i).optString("name"));final JSONArray choices=receivers;
        new AlertDialog.Builder(activity).setTitle("Desktop Google Cast output").setItems(names.toArray(new String[0]),(dialog,n)->{if(n==0)googleCommand("discover",null,null);else if(n==1)googleCommand("stop",null,null);else if(n==2)googleSpeakers();else if(choices!=null){String server=activity.getSharedPreferences("settings",0).getString("cast-server","");if(server.isEmpty()){status.setText("Choose an AMP Cast server on the Cast screen first");return;}try{URI uri=new URI(new CastClient(server,"").base);String host=uri.getHost();if(host==null)host=server;googleCommand("start",choices.optJSONObject(n-3).optString("id"),host+":"+(uri.getPort()>0?uri.getPort():8787));}catch(Exception e){status.setText("Choose a valid Cast server address");}}}).setNegativeButton("Close",null).show();
    }
    private void command(String action,Object...fields){try{JSONObject body=new JSONObject().put("id","android-"+UUID.randomUUID()).put("action",action);for(int i=0;i<fields.length;i+=2)body.put(fields[i].toString(),fields[i+1]);String endpoint=base,credential=secret,identity=session;if(endpoint.isEmpty()||!connected)throw new IllegalArgumentException("Connect first");submit(()->{JSONObject reply=request(endpoint,credential,"/api/v1/command",body,identity);if(!reply.optBoolean("ok"))throw new IOException(reply.optString("error","Command rejected"));return request(endpoint,credential,"/api/v1/state",null);},state->{status.setText("Accepted · "+action);showState(state);});}catch(Exception error){status.setText(error.getMessage());}}
    private void cue(int side,String operation,Object...fields){try{JSONObject value=new JSONObject().put("operation",operation);for(int i=0;i<fields.length;i+=2)value.put(fields[i].toString(),fields[i+1]);command("cue_deck","side",side,"operation",value);}catch(Exception error){status.setText(error.getMessage());}}
    private void loadPlaylist(JSONObject p){if(p.optString("id").startsWith("regular_"))command("load_playlist","playlist",p.optString("id"));else command("load_smart_playlist","playlist",p.optString("id"),"execution","load_only");}
    private void choosePlaylist(){String[] names=new String[playlists.length()];for(int i=0;i<names.length;i++)names[i]=playlists.optJSONObject(i).optString("name");new AlertDialog.Builder(activity).setTitle("Load desktop playlist").setItems(names,(d,i)->loadPlaylist(playlists.optJSONObject(i))).setNegativeButton("Cancel",null).show();}
    private void renderSongs(){songs.removeAllViews();String query=search.getText().toString().toLowerCase(Locale.ROOT);int count=0;for(int i=0;i<catalog.length();i++){JSONObject song=catalog.optJSONObject(i);if(song==null||!song.optString("title").toLowerCase(Locale.ROOT).contains(query))continue;if(count++>=100)break;songs.addView(button(song.optString("title"),()->new AlertDialog.Builder(activity).setTitle(song.optString("title")).setItems(new String[]{"Load → A","Load → B","Load left Cue Deck","Load right Cue Deck","Saved cues","Request in next 1–5 songs"},(d,n)->{if(n<2)command("call_song","song",song.optString("id"),"deck",n==0?"a":"b","execution","load_only");else if(n<4)cue(n-2,"load","song",song.optString("id"));else if(n==5){new AlertDialog.Builder(activity).setTitle("Play within how many songs?").setItems(new String[]{"1","2","3","4","5"},(a,j)->command("request_song","song",song.optString("id"),"within",j+1)).show();}else{JSONArray cues=song.optJSONArray("cues");if(cues==null)cues=new JSONArray();final JSONArray saved=cues;String[] names=new String[cues.length()];for(int j=0;j<names.length;j++)names[j]=cues.optJSONObject(j).optString("name")+" · "+cues.optJSONObject(j).optDouble("seconds");new AlertDialog.Builder(activity).setTitle("Load saved cue → "+deck.toUpperCase(Locale.ROOT)).setItems(names,(a,j)->command("call_song","song",song.optString("id"),"cue",saved.optJSONObject(j).optString("id"),"deck",deck,"execution","load_only")).show();}}).show()));}if(count>100)songs.addView(label("First 100 matches · refine search",13));}
    static JSONObject request(String base,String secret,String path,JSONObject body)throws Exception {return request(base,secret,path,body,"");}
    static JSONObject request(String base,String secret,String path,JSONObject body,String identity)throws Exception {
        HttpURLConnection connection=(HttpURLConnection)new URL(base+path).openConnection();connection.setInstanceFollowRedirects(false);connection.setConnectTimeout(4000);connection.setReadTimeout(7000);if(!secret.isEmpty())connection.setRequestProperty("x-amp-password",secret);if(!identity.isEmpty())connection.setRequestProperty("x-amp-session",identity);
        try{if(body!=null){connection.setRequestMethod("POST");connection.setDoOutput(true);connection.setRequestProperty("Content-Type","application/json");try(OutputStream output=connection.getOutputStream()){output.write(body.toString().getBytes(StandardCharsets.UTF_8));}}int code=connection.getResponseCode();if(code<200||code>=300){String reason=code==401||code==403?"Incorrect password or host session changed. Connect again.":"desktop rejected this request";InputStream failure=connection.getErrorStream();if(failure!=null)try(InputStream input=failure;ByteArrayOutputStream message=new ByteArrayOutputStream()){byte[] buffer=new byte[512];int count;while(message.size()<4096&&(count=input.read(buffer,0,Math.min(buffer.length,4096-message.size())))!=-1)message.write(buffer,0,count);String text=message.toString("UTF-8");try{reason=new JSONObject(text).optString("error",reason);}catch(JSONException ignored){if(!text.isEmpty())reason=text.substring(0,Math.min(180,text.length()));}}throw new IOException("HTTP "+code+": "+reason);}try(InputStream input=connection.getInputStream();ByteArrayOutputStream output=new ByteArrayOutputStream()){byte[] bytes=new byte[8192];int n;while((n=input.read(bytes))!=-1){if(output.size()+n>8*1024*1024)throw new IOException("Catalog too large");output.write(bytes,0,n);}return new JSONObject(output.toString("UTF-8"));}}finally{connection.disconnect();}
    }
    void close(){closed=true;generation++;discoveryGeneration++;discoveryWorkers.shutdownNow();handler.removeCallbacksAndMessages(null);if(socket!=null)socket.cancel();socketClient.dispatcher().executorService().shutdown();socketClient.connectionPool().evictAll();worker.shutdownNow();}
}
