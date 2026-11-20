package cloud.gitwide.ampverse;
import android.app.AlertDialog;
import android.widget.LinearLayout;
import org.json.*;
/** Same host flow as Full; only phone playback/storage crosses AMP Player IPC. */
final class FeatureScreen {
    final CompanionActivity a;
    CastPanel panel;
    String job="";
    boolean closed;
    FeatureScreen(CompanionActivity a){this.a=a;}
    JSONObject args(Object... pairs){JSONObject j=new JSONObject();try{for(int i=0;i<pairs.length;i+=2)j.put(String.valueOf(pairs[i]),pairs[i+1]);}catch(JSONException ignored){}return j;}
    boolean phoneReady(){if(a.playerReady)return true;a.status.setText("AMP Player IPC unavailable. Open/update the matching AMP Player; server controls work independently.");return false;}
    void open(LinearLayout parent){
        panel=new CastPanel(a,a.colors,new CastPanel.Listener(){
            public boolean phoneAvailable(){return a.playerReady;}
            public void listen(String url,String name){if(phoneReady())a.command("player_stream",args("url",url,"title",name));}
            public void stopListening(){if(phoneReady())a.command("player_stop_radio",null);}
        }){
            @Override void offline(){FeatureScreen.this.offline();}
            @Override void downloadPlaylist(CastClient selected,JSONObject manifest,JSONObject playlist){
                if(!selected(selected)||!phoneReady())return;
                a.rpc("sync_cast",args("address",selected.base,"password",password.getText().toString(),"playlist",playlist.optString("id")),result->{job=result.getString("job");poll();});
            }
        };
        parent.addView(panel);
        a.ipcChanged=()->{if(!closed)panel.refreshAvailability();};
        parent.addView(a.button("Cancel playlist download",()->{if(!job.isEmpty())a.command("job",args("job",job,"cancel",true));}));
    }
    void poll(){if(closed||job.isEmpty())return;a.rpc("job",args("job",job),result->{a.status.setText("AMP Player download: "+result.optString("status")+" "+result.optString("error"));if(!result.optBoolean("done"))a.handler.postDelayed(this::poll,1000);});}
    void offline(){if(!phoneReady())return;a.rpc("offline_lists",null,result->{JSONArray items=result.getJSONArray("items");String[] names=new String[items.length()];for(int i=0;i<names.length;i++)names[i]=items.getJSONObject(i).optString("name");if(names.length==0){a.status.setText("AMP Player: no offline playlists. Connect to a server and choose Save offline with my cues.");return;}new AlertDialog.Builder(a).setTitle("Offline playlists in AMP Player").setItems(names,(d,i)->{if(phoneReady())a.command("offline_play",args("id",items.optJSONObject(i).optString("id")));}).setNegativeButton("Close",null).show();});}
    void close(){closed=true;if(panel!=null)panel.close();a.ipcChanged=()->{};}
}
