package cloud.gitwide.ampverse;

import android.app.*;
import android.widget.*;
import androidx.media3.common.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.function.Consumer;
import org.json.*;

/** Bounded song picker. Server selection is read-only; loading never starts the host program. */
final class DjSongPicker {
    private final Activity activity;
    private final Appearance colors;
    private final List<MediaItem> local;
    private final Consumer<MediaItem> load;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final android.os.Handler handler=new android.os.Handler(android.os.Looper.getMainLooper());
    private final CastHostSession hosts;
    private final List<MediaItem> localMatches=new ArrayList<>();
    private JSONArray serverMatches=new JSONArray();
    private CastClient owner;
    private JSONObject identity;
    private TextView status;
    private EditText query;
    private ArrayAdapter<String> adapter;
    private boolean server,closed,loading;
    private int generation;
    private Runnable pending;

    DjSongPicker(Activity activity,Appearance colors,List<MediaItem> local,Consumer<MediaItem> load){this.activity=activity;this.colors=colors;this.local=local;this.load=load;hosts=CastHostSession.get(activity);}
    void show(int deck){
        LinearLayout view=new LinearLayout(activity);view.setOrientation(LinearLayout.VERTICAL);
        int margin=(int)(12*activity.getResources().getDisplayMetrics().density);view.setPadding(margin,margin,margin,margin);
        LinearLayout sources=new LinearLayout(activity);view.addView(sources);
        Button phone=new Button(activity),host=new Button(activity);phone.setText("Phone / offline");host.setText("Server library");
        sources.addView(phone,new LinearLayout.LayoutParams(0,-2,1));sources.addView(host,new LinearLayout.LayoutParams(0,-2,1));
        query=new EditText(activity);query.setSingleLine(true);query.setHint("Search songs");query.setContentDescription("Search songs to load into DJ");view.addView(query);
        status=new TextView(activity);status.setTextColor(colors.text);view.addView(status);
        ListView list=new ListView(activity);list.setContentDescription("DJ load results");adapter=new ArrayAdapter<>(activity,android.R.layout.simple_list_item_1,new ArrayList<>());list.setAdapter(adapter);view.addView(list,new LinearLayout.LayoutParams(-1,(int)(300*activity.getResources().getDisplayMetrics().density)));
        AlertDialog dialog=new AlertDialog.Builder(activity).setTitle("Load "+(deck<2?"Deck "+(deck==0?"A":"B"):"Cue Deck")).setView(view).setNegativeButton("Cancel",null).create();
        phone.setOnClickListener(v->{server=false;changed();});host.setOnClickListener(v->{server=true;owner=hosts.client();identity=null;changed();});
        query.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence s,int start,int count,int after){}public void onTextChanged(CharSequence s,int start,int before,int count){changed();}public void afterTextChanged(android.text.Editable e){}});
        list.setOnItemClickListener((p,v,index,id)->{
            if(loading)return;
            if(!server){if(index<localMatches.size()){load.accept(localMatches.get(index));dialog.dismiss();}return;}
            JSONObject row=serverMatches.optJSONObject(index);if(row==null)return;
            final int request=++generation;final CastClient captured=owner;final JSONObject capturedIdentity=identity;
            loading=true;status.setText("Preparing selected server song…");
            worker.execute(()->{try{
                requireOwner(captured);JSONObject args=new JSONObject().put("server_id",capturedIdentity.getString("server_id")).put("library_id",capturedIdentity.getString("library_id")).put("tracks",new JSONArray().put(CastLibraryModel.catalogPath(row.getString("path"))));
                captured.withPrivateAuthorization((base,password,s,l)->{try{args.put("base",base).put("password",password);}catch(JSONException e){throw new IllegalStateException(e);}});
                JSONObject prepared=PrivateLibraryPlayback.prepare(args);requireOwner(captured);
                List<MediaItem> songs=PrivateLibraryPlayback.take(prepared.getString("token"));
                MediaItem item=songs.get(0).buildUpon().setMediaMetadata(songs.get(0).mediaMetadata.buildUpon().setTitle(row.optString("title",row.getString("path"))).build()).build();
                ui(request,()->{load.accept(item);dialog.dismiss();});
            }catch(Exception error){ui(request,()->{loading=false;status.setText("Could not load song: "+message(error));});}});
        });
        dialog.setOnDismissListener(d->{closed=true;generation++;if(pending!=null)handler.removeCallbacks(pending);worker.shutdownNow();});
        server=local.isEmpty()&&hosts.client()!=null;owner=hosts.client();dialog.show();changed();
    }
    private void requireOwner(CastClient captured){if(closed||captured==null||hosts.client()!=captured)throw new IllegalStateException("Server changed. Reopen Load from DJ.");}
    private void ui(int request,Runnable action){activity.runOnUiThread(()->{if(!closed&&request==generation)action.run();});}
    private static String message(Exception error){return error.getMessage()==null?"server unavailable":error.getMessage();}
    private void changed(){
        final int request=++generation;loading=false;if(pending!=null)handler.removeCallbacks(pending);adapter.clear();localMatches.clear();serverMatches=new JSONArray();
        String text=query.getText().toString().trim();
        if(!server){for(MediaItem item:local){String title=String.valueOf(item.mediaMetadata.title);if(title.toLowerCase(Locale.ROOT).contains(text.toLowerCase(Locale.ROOT))&&localMatches.size()<200){localMatches.add(item);adapter.add(title);}}status.setText(localMatches.isEmpty()?"No phone songs. Choose Server library to stream, or add music in Library.":localMatches.size()+" phone / offline songs");return;}
        final CastClient captured=owner;if(captured==null){status.setText("Connect to your server in Devices, then reopen Load. Phone music remains available.");return;}
        status.setText("Searching selected server…");
        pending=()->worker.execute(()->{try{
            requireOwner(captured);JSONObject verified=captured.status();
            JSONObject result=captured.get("/api/studio/tracks?offset=0&limit=50&q="+android.net.Uri.encode(text));
            requireOwner(captured);JSONObject after=captured.status();
            if(!verified.optString("library_id").equals(after.optString("library_id")))throw new IllegalStateException("Server library changed; reconnect");
            JSONArray rows=result.getJSONArray("tracks");if(rows.length()>50)throw new IllegalStateException("Invalid server search page");
            for(int i=0;i<rows.length();i++)CastLibraryModel.catalogPath(rows.getJSONObject(i).getString("path"));
            ui(request,()->{identity=verified;serverMatches=rows;adapter.clear();for(int i=0;i<rows.length();i++){JSONObject row=rows.optJSONObject(i);adapter.add(row.optString("title",row.optString("path")));}status.setText(rows.length()+" of "+result.optInt("total")+" server songs · refine search");});
        }catch(Exception error){ui(request,()->status.setText("Server search unavailable: "+message(error)));}});
        handler.postDelayed(pending,300);
    }
}
