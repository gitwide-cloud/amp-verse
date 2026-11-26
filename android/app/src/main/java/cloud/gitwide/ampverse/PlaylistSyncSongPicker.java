package cloud.gitwide.ampverse;
import android.app.*;
import android.os.*;
import android.widget.*;
import android.text.*;
import java.util.*;
import java.util.concurrent.*;
import org.json.*;

/** Read-only, debounced whole-server search. Only an explicit selection is cached. */
final class PlaylistSyncSongPicker {
    interface Choice {void selected(JSONObject reference)throws Exception;}
    static void show(Activity activity,JSONObject state,String title,Choice choice){new Picker(activity,state,title,choice).show();}
    private static final class Picker {
        final Activity activity;final JSONObject captured;final Choice choice;
        final Handler main=new Handler(Looper.getMainLooper());
        final ScheduledExecutorService worker=Executors.newSingleThreadScheduledExecutor(r->{Thread t=new Thread(r,"amp-playlist-search");t.setDaemon(true);return t;});
        final ArrayList<String> ids=new ArrayList<>(),labels=new ArrayList<>();
        final EditText search;final TextView note;final ListView list;final ArrayAdapter<String> adapter;final AlertDialog dialog;
        volatile boolean closed;long generation;ScheduledFuture<?> scheduled;
        PlaylistSyncSearch.Cancel cancel=new PlaylistSyncSearch.Cancel();PlaylistSyncSearch.Result result;
        Picker(Activity activity,JSONObject state,String title,Choice choice){
            this.activity=activity;this.captured=state;this.choice=choice;
            LinearLayout body=new LinearLayout(activity);body.setOrientation(LinearLayout.VERTICAL);
            search=new EditText(activity);search.setSingleLine(true);search.setHint("Search entire server library");body.addView(search);
            note=new TextView(activity);body.addView(note);list=new ListView(activity);body.addView(list,new LinearLayout.LayoutParams(-1,(int)(320*activity.getResources().getDisplayMetrics().density)));
            adapter=new ArrayAdapter<>(activity,android.R.layout.simple_list_item_1,labels);list.setAdapter(adapter);
            dialog=new AlertDialog.Builder(activity).setTitle(title).setMessage("Choose the exact server song. Relative paths distinguish versions. Search does not publish a playlist.").setView(body).setNegativeButton("Cancel",null).create();
            dialog.setOnDismissListener(d->close());
            list.setOnItemClickListener((parent,view,index,id)->choose(index));
            search.addTextChangedListener(new TextWatcher(){public void beforeTextChanged(CharSequence s,int start,int count,int after){}public void onTextChanged(CharSequence s,int start,int before,int count){query();}public void afterTextChanged(Editable e){}});
        }
        void show(){dialog.show();query();}
        boolean current(long token,PlaylistSyncSearch.Cancel request){return !closed&&!activity.isFinishing()&&!activity.isDestroyed()&&generation==token&&!request.cancelled();}
        void close(){closed=true;generation++;cancel.cancel();if(scheduled!=null)scheduled.cancel(false);worker.shutdownNow();}
        void query(){
            if(closed)return;long token=++generation;cancel.cancel();cancel=new PlaylistSyncSearch.Cancel();final PlaylistSyncSearch.Cancel request=cancel;
            if(scheduled!=null)scheduled.cancel(false);result=null;ids.clear();labels.clear();adapter.notifyDataSetChanged();note.setText("Searching the selected server…");
            String text=search.getText().toString().trim();
            scheduled=worker.schedule(()->{try{
                PlaylistSyncSearch.Result found=PlaylistSync.searchCatalog(activity.getApplicationContext(),captured,text,request);
                main.post(()->{if(!current(token,request))return;try{result=found;Iterator<String> keys=found.entries.keys();while(keys.hasNext()){String key=keys.next();JSONObject ref=found.entries.getJSONObject(key);ids.add(key);labels.add(ref.optString("title",ref.getString("relative_path"))+"\n"+ref.getString("relative_path"));}adapter.notifyDataSetChanged();note.setText(found.total==0?"No matching server songs. Try another title, artist, album or path.":"Showing "+ids.size()+" of "+found.total+" matches"+(found.total>200?". Refine the search to find the exact version.":". Choose explicitly; nothing is matched automatically."));}catch(Exception error){unavailable();}});
            }catch(Exception error){main.post(()->{if(current(token,request))unavailable();});}},300,TimeUnit.MILLISECONDS);
        }
        void unavailable(){note.setText("Search unavailable. Check the connection or reconnect the selected server, then try again. No playlist was published.");}
        void choose(int index){
            if(result==null||index<0||index>=ids.size()||closed)return;
            final long token=generation;final PlaylistSyncSearch.Cancel request=cancel;final PlaylistSyncSearch.Result selected=result;final String key=ids.get(index);
            list.setEnabled(false);search.setEnabled(false);note.setText("Verifying the selected reference…");
            worker.execute(()->{try{JSONObject reference=PlaylistSync.rememberSearchChoice(activity.getApplicationContext(),captured,selected,key,request);
                main.post(()->{if(!current(token,request))return;dialog.dismiss();try{choice.selected(reference);}catch(Exception error){Toast.makeText(activity,"Selection could not be applied. Reopen the saved playlist and retry.",Toast.LENGTH_LONG).show();}});
            }catch(Exception error){main.post(()->{if(current(token,request)){list.setEnabled(true);search.setEnabled(true);unavailable();}});}});
        }
    }
}
