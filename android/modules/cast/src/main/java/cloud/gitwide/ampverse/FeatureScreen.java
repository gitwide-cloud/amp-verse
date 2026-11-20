package cloud.gitwide.ampverse;
import android.app.*;
import android.content.Intent;
import android.widget.*;
import org.json.*;
final class FeatureScreen {
    final CompanionActivity a;
    LinearLayout servers,lists,connectionFields;
    EditText address,password;
    CastClient client;
    String title="AMP station",job="";
    boolean closed;
    FeatureScreen(CompanionActivity a){this.a=a;}
    JSONObject args(Object... pairs){JSONObject j=new JSONObject();try{for(int i=0;i<pairs.length;i+=2)j.put(String.valueOf(pairs[i]),pairs[i+1]);}catch(Exception ignored){}return j;}
    void open(LinearLayout parent){
        a.row(parent,a.button("Find LAN stations",this::discover),a.button("Offline playlists",this::offline));
        servers=a.column();parent.addView(servers);
        parent.addView(a.button("Advanced connection settings", () -> connectionFields.setVisibility(
                connectionFields.getVisibility() == android.view.View.VISIBLE
                        ? android.view.View.GONE : android.view.View.VISIBLE)));
        connectionFields = a.column();
        connectionFields.setVisibility(android.view.View.GONE);
        parent.addView(connectionFields);
        address=new EditText(a);address.setSingleLine(true);address.setHint("AMP server address");address.setTextColor(a.colors.text);address.setText(a.getSharedPreferences("settings",0).getString("cast-server",""));connectionFields.addView(address);
        password=new EditText(a);password.setSingleLine(true);password.setHint("Optional server password");password.setInputType(129);password.setTextColor(a.colors.text);connectionFields.addView(password);
        connectionFields.addView(a.button("Connect", () -> connect(null)));
        a.row(parent,a.button("Listen on phone",()->{if(client!=null)a.command("player_stream",args("url",client.phoneStream(),"title",title));}),a.button("Stop phone",()->a.command("player_pause",null)));
        parent.addView(a.button("Google Home / Chromecast",()->{if(client==null){a.status.setText("Connect to a station first");return;}a.startActivity(new Intent(a,GoogleCastActivity.class).putExtra("stream",client.stream()).putExtra("title",title));}));
        a.row(parent,a.button("Start station",()->station(true)),a.button("Stop station",()->station(false)));
        parent.addView(a.button("Broadcast host DJ / Player",()->{if(client!=null)a.work(()->{client.post("/api/live/launch",new JSONObject());return "Live master mix requested on the selected host";});}));
        lists=a.column();parent.addView(lists);
        parent.addView(a.button("Cancel playlist download",()->{if(!job.isEmpty())a.command("job",args("job",job,"cancel",true));}));
    }
    void discover() {
        a.work(() -> {
            java.util.List<CastDiscovery.Server> found = CastDiscovery.find();
            a.runOnUiThread(() -> {
                if (closed) return;
                servers.removeAllViews();
                for (CastDiscovery.Server server : found) {
                    servers.addView(a.button(server.name, () -> {
                        address.setText(server.url);
                        password.setText("");
                        connect(server.id);
                    }));
                }
            });
            return found.isEmpty()
                    ? "No stations found. Check that both devices are on the same private network."
                    : "Choose a station.";
        });
    }

    void connect(String expectedIdentity) {
        String url = address.getText().toString();
        String secret = password.getText().toString();
        a.work(() -> {
            CastClient selected = new CastClient(url, secret);
            JSONObject state = selected.connect();
            String serverId = state.optString("server_id");
            String stationId = state.optString("station_id");
            if (expectedIdentity != null && !expectedIdentity.equals(serverId + "/" + stationId)) {
                throw new java.io.IOException("Station changed. Search again before connecting.");
            }
            String name = state.optString("station_name", "AMP station");
            a.runOnUiThread(() -> {
                if (closed) return;
                client = selected;
                title = name;
                connectionFields.setVisibility(android.view.View.GONE);
                a.getSharedPreferences("settings", 0).edit().putString("cast-server", selected.base)
                        .putString("cast-server-id", serverId).putString("cast-station-id", stationId).apply();
                lists.removeAllViews();
            });
            JSONObject capabilities = state.optJSONObject("capabilities");
            if (capabilities == null || !capabilities.optBoolean("smart_sharing")) {
                return "Connected to " + name + " · offline playlist sharing is off.";
            }
            JSONObject manifest;
            try {
                manifest = selected.get("/api/smart/manifest");
            } catch (java.io.IOException error) {
                return "Connected to " + name + " · offline playlists are unavailable.";
            }
            a.runOnUiThread(() -> showPlaylists(selected, secret, manifest));
            return "Connected to " + name;
        });
    }

    private void showPlaylists(CastClient selected, String secret, JSONObject manifest) {
        if (closed || client != selected) return;
        JSONArray playlists = manifest.optJSONArray("playlists");
        if (playlists == null) return;
        for (int i = 0; i < playlists.length(); i++) {
            JSONObject list = playlists.optJSONObject(i);
            if (list == null) continue;
            String id = list.optString("id");
            lists.addView(a.label(list.optString("name"), 18));
            a.row(lists, a.button("Play station", () -> a.work(() -> {
                selected.playPlaylist(id);
                return "Station playlist selected";
            })), a.button("Save offline + cues", () -> a.rpc("sync_cast",
                    args("address", selected.base, "password", secret, "playlist", id), result -> {
                        job = result.getString("job");
                        poll();
                    })));
        }
    }
    void station(boolean start){if(client!=null)a.work(()->{client.command(start);return start?"Station started":"Station stopped";});}
    void poll(){if(closed||job.isEmpty())return;a.rpc("job",args("job",job),result->{a.status.setText(result.optString("status")+" "+result.optString("error"));if(!result.optBoolean("done"))a.handler.postDelayed(this::poll,1000);});}
    void offline(){a.rpc("offline_lists",null,result->{JSONArray items=result.getJSONArray("items");String[] names=new String[items.length()];for(int i=0;i<names.length;i++)names[i]=items.getJSONObject(i).optString("name");if(names.length==0){a.status.setText("No offline playlists. Connect to a host and choose Save offline + cues.");return;}new AlertDialog.Builder(a).setTitle("Offline playlists in AMP Player").setItems(names,(d,i)->a.command("offline_play",args("id",items.optJSONObject(i).optString("id")))).setNegativeButton("Close",null).show();});}
    void close(){closed=true;}
}
