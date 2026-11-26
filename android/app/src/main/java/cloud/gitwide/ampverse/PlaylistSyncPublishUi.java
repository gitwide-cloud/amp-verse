package cloud.gitwide.ampverse;
import android.app.*;
import android.widget.*;
import android.text.*;
import androidx.media3.common.MediaItem;
import java.util.*;
import org.json.*;

/** Reviewed copy publication; the original phone draft is never edited/deleted. */
final class PlaylistSyncPublishUi {
    static void show(Activity a,JSONObject original){try{
        JSONObject local=PlaylistSyncModel.copy(original),state=PlaylistSync.publicationState(a);
        if(local.optBoolean("smart"))new AlertDialog.Builder(a).setTitle("Choose this copy's server source").setMessage("Phone folders and phone playlists cannot be assumed to match server folders. Saved genres and filters will be preserved. Choose the server source explicitly.")
            .setPositiveButton("Server library",(d,n)->match(a,local,state,true,new JSONObject(),0))
            .setNeutralButton("Matched phone selection",(d,n)->match(a,local,state,false,new JSONObject(),0)).setNegativeButton("Cancel",null).show();
        else match(a,local,state,false,new JSONObject(),0);
    }catch(Exception error){error(a,error);}}
    private static void error(Activity a,Exception error){Toast.makeText(a,error.getMessage(),Toast.LENGTH_LONG).show();}
    private static void match(Activity a,JSONObject local,JSONObject state,boolean all,JSONObject selected,int at){try{
        JSONArray required=PlaylistSyncDraft.required(local,all);
        if(at>=required.length()){review(a,local,state,all,selected);return;}
        String phone=required.getString(at),title="Unmatched phone song "+(at+1);for(MediaItem song:LibraryStore.read(a))if(song.mediaId.equals(phone)){title=String.valueOf(song.mediaMetadata.title);break;}
        PlaylistSyncSongPicker.show(a,state,"Match phone song: "+title,reference->{
            String key=reference.getString("track_key");state.getJSONObject("catalog").put(key,PlaylistSyncModel.copy(reference));selected.put(phone,key);
            match(a,local,state,all,selected,at+1);
        });
    }catch(Exception error){error(a,error);}}
    private static void review(Activity a,JSONObject local,JSONObject state,boolean all,JSONObject selected)throws Exception {
        JSONObject prepared=PlaylistSyncDraft.prepare(local,state,selected,all,"review");JSONObject document=prepared.getJSONObject("document");
        String summary="Publish a new server copy of “"+local.getString("name")+"”?\n\n"+(local.optBoolean("smart")?"Source: "+(all?"server library":"explicit matched phone selection")+"; "+(document.optJSONArray("genres")==null?0:document.optJSONArray("genres").length())+" genres; "+(document.optJSONArray("filters")==null?0:document.optJSONArray("filters").length())+" filters.":document.getJSONArray("tracks").length()+" ordered server references; duplicates retained.")+"\n\nThe original phone-only playlist and its files remain unchanged. This queues a draft; it is not delivered until the server acknowledges it. Other clients must connect to receive it.";
        new AlertDialog.Builder(a).setTitle("Review server publication").setMessage(summary).setPositiveButton("Queue server copy",(d,n)->{try{PlaylistSync.publishLocal(a,state.getString("server_id"),state.getString("library_id"),state.getLong("_publication_generation"),local,selected,all);}catch(Exception e){error(a,e);}}).setNegativeButton("Cancel",null).show();
    }
}
