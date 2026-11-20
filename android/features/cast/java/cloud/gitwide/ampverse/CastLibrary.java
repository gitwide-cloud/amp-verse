package cloud.gitwide.ampverse;

import android.app.AlertDialog;
import android.content.Intent;
import android.net.Uri;
import android.widget.*;
import org.json.*;
import java.util.*;

/** Selected server library. Browsing is read-only; saves and playback are explicit actions. */
final class CastLibrary extends LinearLayout {
    final CastPanel panel;
    CastClient owner;
    JSONArray playlists=new JSONArray();
    JSONObject selection;
    final TextView title,selected,downloadNote;
    final EditText query;
    final LinearLayout rows;
    final Button browse,create,play,stop,download,remote,search,previous,next;
    final LinkedHashSet<Integer> chosen=new LinkedHashSet<>();
    int offset,total;
    String filter="";
    boolean ready;
    CastLibrary(CastPanel panel){
        super(panel.activity);this.panel=panel;setOrientation(VERTICAL);
        title=panel.label("Library",20);addView(title);
        remote=panel.action("Remote",()->{if(!valid())return;try{panel.activity.startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse(owner.base+"/")));}catch(android.content.ActivityNotFoundException e){panel.status.setText("Install a browser to open this server’s web player.");}});addView(remote);
        browse=panel.action("Browse playlists",this::browse);addView(browse);
        selected=panel.label("Select a playlist to play or download. Select library songs below to create one.",14);addView(selected);
        play=panel.action("Play selected playlist",()->{if(!valid()||selection==null)return;final CastClient target=owner;final String id=selection.optString("id");new AlertDialog.Builder(panel.activity).setTitle("Play on this station?").setMessage("This replaces the selected server’s current program for all listeners. Choose Listen on this phone above to hear it here.").setPositiveButton("Play",(d,n)->{if(!valid()||owner!=target)return;panel.work(()->{target.status();target.post("/api/station/play/"+Uri.encode(id),null);panel.sync(target);return "Playlist requested on server. Phone listening is a separate output.";});}).setNegativeButton("Cancel",null).show();});addView(play);
        stop=panel.action("Stop station playback",()->{if(!valid())return;final CastClient target=owner;new AlertDialog.Builder(panel.activity).setTitle("Stop this station?").setMessage("All listeners lose this station’s audio. Stop phone listening instead to leave the station running.").setPositiveButton("Stop station",(d,n)->{if(!valid()||owner!=target)return;panel.work(()->{target.status();target.command(false);panel.sync(target);return "Station stopped";});}).setNegativeButton("Cancel",null).show();});addView(stop);
        download=panel.action("Download selected playlist",this::download);addView(download);
        downloadNote=panel.label("",14);addView(downloadNote);
        addView(panel.label("Download copies songs and cues to AMP Player. Download again to refresh the copy. Phone edits are not uploaded; two-way sync is not implemented.",14));
        create=panel.action("Create playlist on server",this::create);addView(create);
        int songsStart=getChildCount();
        query=new EditText(panel.activity);query.setSingleLine(true);query.setTextColor(panel.colors.text);query.setHintTextColor(panel.colors.muted);query.setHint("Search this server’s library");addView(query);
        search=panel.action("Search library",()->page(0,query.getText().toString().trim()));addView(search);
        rows=new LinearLayout(panel.activity);rows.setOrientation(VERTICAL);addView(rows);
        previous=panel.action("Previous songs",()->page(Math.max(0,offset-50),filter));addView(previous);
        next=panel.action("More songs",()->page(offset+50,filter));addView(next);
        UiSections.fold(this,panel.colors,"Choose songs for a new playlist",songsStart);
        setVisibility(GONE);
    }
    static JSONObject snapshot(CastClient client)throws Exception {
        return new JSONObject().put("playlists",client.get("/api/studio/playlists").getJSONArray("playlists")).put("page",client.get("/api/studio/tracks?limit=50"));
    }
    void open(CastClient client,String name,JSONObject data){owner=client;selection=null;chosen.clear();create.setText("Create playlist on server");filter="";query.setText("");playlists=data.optJSONArray("playlists");if(playlists==null)playlists=new JSONArray();title.setText(name+" · Library");selected.setText("Select a playlist to play or download. Select library songs below to create one.");render(data.optJSONObject("page"));setVisibility(VISIBLE);}
    void clear(){owner=null;selection=null;chosen.clear();rows.removeAllViews();setVisibility(GONE);}
    boolean valid(){return owner!=null&&panel.selected(owner);}
    void availability(boolean value){ready=value;downloadNote.setText(!panel.sharingSupported?"Downloads are off on this server. Enable playlist sharing in Advanced or ask its owner.":!panel.listener.phoneAvailable()?"Connect the matching AMP Player to download.":selection!=null&&(selection.optJSONArray("tracks")==null||selection.optJSONArray("tracks").length()==0)?"This playlist is empty. It cannot play or download yet.":"Download is a one-way copy; phone edits are not uploaded.");for(Button b:new Button[]{browse,create,play,stop,download,remote,search,previous,next}){boolean enabled=value;if(b==play||b==download)enabled&=selection!=null;if(b==download)enabled&=panel.sharingSupported&&panel.listener.phoneAvailable()&&selection!=null&&selection.optJSONArray("tracks")!=null&&selection.optJSONArray("tracks").length()>0;if(b==stop)enabled&=panel.running||panel.preparing;if(b==play)enabled&=panel.playlistSupported&&selection!=null&&selection.optJSONArray("tracks")!=null&&selection.optJSONArray("tracks").length()>0;if(b==previous)enabled&=offset>0;if(b==next)enabled&=offset+50<total;b.setEnabled(enabled);b.setAlpha(enabled?1f:.35f);}for(int i=0;i<rows.getChildCount();i++)rows.getChildAt(i).setEnabled(value);}
    void render(JSONObject page){if(page==null)return;offset=page.optInt("offset");total=page.optInt("total");rows.removeAllViews();JSONArray songs=page.optJSONArray("tracks");if(songs!=null)for(int i=0;i<songs.length();i++){JSONObject song=songs.optJSONObject(i);if(song==null)continue;int id=song.optInt("id",-1);if(id<0)continue;CheckBox row=new CheckBox(panel.activity);row.setTextColor(panel.colors.text);row.setButtonTintList(android.content.res.ColorStateList.valueOf(panel.colors.text));row.setText(song.optString("title","Track"));row.setChecked(chosen.contains(id));row.setOnCheckedChangeListener((button,on)->{if(on)chosen.add(id);else chosen.remove(id);create.setText(chosen.isEmpty()?"Create playlist on server":"Create playlist on server ("+chosen.size()+" songs)");});rows.addView(row);}if(rows.getChildCount()==0)rows.addView(panel.label("No matching songs",14));availability(ready);}
    void page(int start,String text){if(!valid())return;final CastClient target=owner;panel.work(()->{target.status();JSONObject data=target.get("/api/studio/tracks?limit=50&offset="+start+"&q="+Uri.encode(text));panel.activity.runOnUiThread(()->{if(!panel.closed&&owner==target){filter=text;render(data);}});return "Library loaded";});}
    void browse(){if(!valid())return;final CastClient target=owner;panel.work(()->{target.status();JSONArray lists=target.get("/api/studio/playlists").getJSONArray("playlists");panel.activity.runOnUiThread(()->{if(panel.closed||owner!=target)return;playlists=lists;String[] names=new String[lists.length()];for(int i=0;i<names.length;i++)names[i]=lists.optJSONObject(i).optString("name");new AlertDialog.Builder(panel.activity).setTitle(names.length==0?"No server playlists yet":"Server playlists").setItems(names,(d,i)->{if(!valid()||owner!=target)return;selection=lists.optJSONObject(i);JSONArray songs=selection.optJSONArray("tracks");selected.setText(selection.optString("name")+" · "+(songs==null?0:songs.length())+" songs");availability(true);}).setNegativeButton("Close",null).show();});return "Choose a playlist";});}
    void create(){if(!valid())return;if(chosen.isEmpty()){panel.status.setText("Choose at least one song under Choose songs for a new playlist before saving.");return;}final CastClient target=owner;final JSONArray ids=new JSONArray(chosen);EditText name=new EditText(panel.activity);name.setSingleLine(true);name.setContentDescription("New server playlist name");name.setHint("Playlist name");new AlertDialog.Builder(panel.activity).setTitle("Save playlist on server").setMessage(ids.length()+" selected songs. Saving does not start playback or replace an existing playlist.").setView(name).setPositiveButton("Save",(d,n)->{if(!valid()||owner!=target)return;String value=name.getText().toString().trim();if(value.isEmpty()){panel.status.setText("Enter a playlist name");return;}panel.work(()->{target.status();JSONObject saved=target.postJson("/api/studio/playlists",new JSONObject().put("name",value).put("tracks",ids));String id=saved.getString("id");JSONArray lists=target.get("/api/studio/playlists").getJSONArray("playlists");JSONObject found=null;for(int i=0;i<lists.length();i++)if(id.equals(lists.getJSONObject(i).optString("id")))found=lists.getJSONObject(i);if(found==null)throw new java.io.IOException("Save acknowledged but playlist not found on refresh; browse before retrying");final JSONObject confirmed=found;panel.activity.runOnUiThread(()->{if(!panel.closed&&owner==target){playlists=lists;selection=confirmed;selected.setText(value+" · saved on server");chosen.clear();create.setText("Create playlist on server");for(int i=0;i<rows.getChildCount();i++)if(rows.getChildAt(i) instanceof CheckBox)((CheckBox)rows.getChildAt(i)).setChecked(false);}});return "Playlist saved and read back from server; station unchanged.";});}).setNegativeButton("Cancel",null).show();}
    void download(){if(!valid()||selection==null||!panel.sharingSupported||!panel.listener.phoneAvailable())return;final CastClient target=owner;final String id=selection.optString("id");panel.work(()->{JSONObject state=target.status();if(state.optJSONObject("capabilities")==null||!state.getJSONObject("capabilities").optBoolean("smart_sharing")){panel.activity.runOnUiThread(()->{if(!panel.closed&&owner==target){panel.sharingSupported=false;availability(ready);}});return "Playlist downloads were turned off on this server. Library access is unchanged.";}JSONObject manifest;try{manifest=target.get("/api/smart/manifest");}catch(CastClient.AuthorizationException e){if(e.code!=403)throw e;target.get("/api/studio/playlists");return "Server declined playlist sharing. Library access is still verified; refresh before retrying.";}JSONArray lists=manifest.getJSONArray("playlists");JSONObject found=null;for(int i=0;i<lists.length();i++)if(id.equals(lists.getJSONObject(i).optString("id")))found=lists.getJSONObject(i);if(found==null)throw new java.io.IOException("Playlist not shared for download on this server");final JSONObject item=found,downloadManifest=manifest;panel.activity.runOnUiThread(()->{if(panel.closed||owner!=target)return;new AlertDialog.Builder(panel.activity).setTitle("Download a one-way copy?").setMessage("Copies audio and cues to AMP Player. Does not upload phone changes.").setPositiveButton("Download",(d,n)->{if(valid()&&owner==target)panel.downloadPlaylist(target,downloadManifest,item);}).setNegativeButton("Cancel",null).show();});return "Ready to download a one-way copy.";});}
}
