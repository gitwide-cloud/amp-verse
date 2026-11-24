package cloud.gitwide.ampverse;
import android.app.Activity;
import android.widget.*;
import org.json.*;
/** Full edition stores downloads locally; companion delegates them to AMP Player. */
final class FullCastPanel extends CastPanel {
    FullCastPanel(Activity activity,Appearance colors,Listener listener){super(activity,colors,listener);}
    @Override void downloadPlaylist(CastClient selected,JSONObject manifest,JSONObject playlist){
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
                public void song(int i,int total,String title){activity.runOnUiThread(()->{if(!closed&&!cancel.get()){bar.setMax(Math.max(1,total));bar.setProgress(i-1);detail.setText(i+" / "+total+" · "+title+(RadioOffline.djAvailable(activity)?"\nAudio + prepared cues":"\nAudio + tags"));}});}
            });
            String name=RadioOffline.album(folder).optString("name");String syncSummary=RadioOffline.summary(folder);
            activity.runOnUiThread(()->{if(!closed)new android.app.AlertDialog.Builder(activity).setTitle("Available offline").setMessage(name+"\n"+syncSummary).setNeutralButton("Review sync changes",(d,n)->RadioOffline.reviewConflicts(activity,folder)).setPositiveButton("Open offline playlists",(d,n)->offline()).setNegativeButton("Done",null).show();});
            return name+": "+syncSummary;
        }catch(Exception e){if(cancel.get())return "Download cancelled. Existing offline playlists were kept.";throw e;}
        finally{activity.runOnUiThread(dialog::dismiss);}});
    }
    @Override void offline(){java.util.List<java.io.File> albums=RadioOffline.albums(activity);String[] names=new String[albums.size()];for(int i=0;i<names.length;i++)try{names[i]=RadioOffline.album(albums.get(i)).optString("name");}catch(Exception error){names[i]="Unavailable playlist";}if(names.length==0){new android.app.AlertDialog.Builder(activity).setTitle("No offline playlists yet").setMessage("Listening to a live broadcast does not download its songs. Connect to your AMP Cast host, enable playlist downloads, then choose Save offline with my cues beside a playlist. The songs must finish downloading before they appear here.").setPositiveButton(client==null?"Find stations":"Show downloadable playlists",(d,n)->{if(client==null)findServers();else{refresh();sharedLists.requestRectangleOnScreen(new android.graphics.Rect(0,0,sharedLists.getWidth(),sharedLists.getHeight()),false);}}).setNegativeButton("Close",null).show();return;}new android.app.AlertDialog.Builder(activity).setTitle("Offline station playlists").setItems(names,(d,index)->{try{listener.playlist(RadioOffline.items(albums.get(index)));}catch(Exception error){status.setText(error.getMessage());}}).setNegativeButton("Close",null).show();}
}
