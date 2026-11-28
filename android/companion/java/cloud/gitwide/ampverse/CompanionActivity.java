package cloud.gitwide.ampverse;

import android.app.*;
import android.content.*;
import android.os.*;
import android.widget.*;
import androidx.media3.session.*;
import androidx.media3.session.MediaController;
import com.google.common.util.concurrent.ListenableFuture;
import org.json.JSONObject;
import java.util.concurrent.*;

/** The signed module owns its screen; AMP Player's service owns playback. */
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
public final class CompanionActivity extends Activity {
    final ExecutorService worker=Executors.newSingleThreadExecutor();
    final Handler handler=new Handler(Looper.getMainLooper());
    Appearance colors;
    LinearLayout body;
    TextView status;
    FeatureScreen feature;
    private ListenableFuture<MediaController> connection;
    private MediaController controller;
    boolean closed,active,playerReady;
    Button updateButton;
    TextView updateStatus;
    final Runnable updatePoll=new Runnable(){public void run(){if(closed)return;updateButton.setEnabled(!AppUpdate.busy);updateStatus.setText(AppUpdate.status);if(!AppUpdate.busy)AppUpdate.installPending(CompanionActivity.this);handler.postDelayed(this,750);}};
    @Override public void onResume(){super.onResume();android.content.SharedPreferences p=getSharedPreferences("updates",0);if(p.getBoolean("permission_pending",false)&&getPackageManager().canRequestPackageInstalls())p.edit().putBoolean("permission_pending",false).putBoolean("install",true).apply();handler.post(updatePoll);}
    @Override public void onPause(){handler.removeCallbacks(updatePoll);super.onPause();}
    Runnable ipcChanged=()->{};
    void ipc(boolean ready,String message){playerReady=ready;status.setText("AMP Player IPC: "+message);ipcChanged.run();}
    int dp(int value){return (int)(value*getResources().getDisplayMetrics().density);}
    TextView label(String text,int size){TextView v=new TextView(this);v.setText(text);v.setTextSize(size);v.setTextColor(colors.text);v.setPadding(dp(6),dp(6),dp(6),dp(6));return v;}
    Button button(String title,Runnable action){Button b=new Button(this);b.setText(title);b.setAllCaps(false);b.setTextColor(colors.text);b.setMinHeight(dp(48));b.setBackground(Chrome.button(colors,false));b.setOnClickListener(v->action.run());return b;}
    LinearLayout column(){LinearLayout v=new LinearLayout(this);v.setOrientation(LinearLayout.VERTICAL);return v;}
    void row(LinearLayout parent,Button... buttons){LinearLayout row=new LinearLayout(this);for(Button b:buttons)row.addView(b,new LinearLayout.LayoutParams(0,dp(52),1));parent.addView(row);}
    interface Result {void accept(JSONObject value)throws Exception;}
    void rpc(String action,JSONObject data,Result result){rpc(action,data,result,()->{});}
    void rpc(String action,JSONObject data,Result result,Runnable complete){if(closed)return;worker.execute(()->{try{JSONObject response=ModuleRpc.call(this,action,data);runOnUiThread(()->{if(closed)return;try{result.accept(response);}catch(Exception e){status.setText("AMP Player: "+e.getMessage());}finally{complete.run();}});}catch(Exception error){runOnUiThread(()->{try{if(!closed)status.setText("AMP Player: "+error.getMessage());}finally{complete.run();}});}});}
    void command(String action,JSONObject data){rpc(action,data,result->status.setText("AMP Player: "+result.optString("message","Done")));}
    void work(Callable<String> action){worker.execute(()->{try{String message=action.call();runOnUiThread(()->{if(!closed)status.setText(message);});}catch(Exception e){runOnUiThread(()->{if(!closed)status.setText(e.getMessage());});}});}
    @Override public void onCreate(Bundle state){super.onCreate(state);if(BuildConfig.DEBUG&&getIntent().getBooleanExtra("amp-test-keep-awake",false))getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);colors=new Appearance(this);LinearLayout root=column();root.setBackground(Chrome.panel(colors,false));root.setPadding(dp(8),dp(8),dp(8),dp(8));root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(dp(8)+i.getSystemWindowInsetLeft(),i.getSystemWindowInsetTop(),dp(8)+i.getSystemWindowInsetRight(),i.getSystemWindowInsetBottom());return i;});
        row(root,button("ϟ AMP "+BuildConfig.MODULE.toUpperCase(java.util.Locale.ROOT),()->startActivity(new Intent().setClassName("cloud.gitwide.ampverse","cloud.gitwide.ampverse.MainActivity"))),button("Back to Player",()->{startActivity(new Intent().setClassName("cloud.gitwide.ampverse","cloud.gitwide.ampverse.MainActivity"));finish();}));
        updateButton=button("Update this add-on",()->{AppUpdate.start(this);status.setText("Checking official updates…");});root.addView(updateButton);updateStatus=label(AppUpdate.status,13);root.addView(updateStatus);
        status=label("Connecting to AMP Player…",14);root.addView(status);ScrollView scroll=new ScrollView(this);body=column();scroll.addView(body);root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));setContentView(root);
        if(BuildConfig.MODULE.equals("cast")){feature=new FeatureScreen(this);feature.open(body);}
    }
    @Override public void onStart(){super.onStart();active=true;connectCore();}
    private void connectCore(){if(closed||!active||connection!=null)return;try{
        if(getPackageManager().checkSignatures(getPackageName(),"cloud.gitwide.ampverse")!=android.content.pm.PackageManager.SIGNATURE_MATCH)throw new IllegalStateException("Install AMP Player and this module from the same signed release.");
        connection=new MediaController.Builder(this,new SessionToken(this,new ComponentName("cloud.gitwide.ampverse","cloud.gitwide.ampverse.PlaybackService"))).setListener(new MediaController.Listener(){@Override public void onDisconnected(MediaController controller){if(CompanionActivity.this.controller!=controller)return;CompanionActivity.this.controller=null;ipc(false,"disconnected; server connection is separate");if(connection!=null)MediaController.releaseFuture(connection);connection=null;if(active&&!closed)handler.postDelayed(CompanionActivity.this::connectCore,500);}}).buildAsync();
        ListenableFuture<MediaController> pending=connection;pending.addListener(()->{if(connection!=pending||!active||closed)return;try{controller=pending.get();rpc("hello",null,result->{if(connection!=pending||!active||closed)return;if(result.optInt("api")!=1||result.optInt("core")<34)throw new IllegalStateException("Update AMP Player to use this module");if(BuildConfig.MODULE.equals("cast")&&!result.optBoolean("radio_stop"))throw new IllegalStateException("Update AMP Player from this Cast release for safe phone listening controls");if(feature==null){feature=new FeatureScreen(this);feature.open(body);}ipc(true,"connected · phone playback is owned by AMP Player");});}catch(Exception error){ipc(false,"unavailable. Open or update AMP Player, then reopen this app.");}},getMainExecutor());
    }catch(Exception error){ipc(false,error.getMessage());body.addView(button("Open AMP Player",()->startActivity(new Intent().setClassName("cloud.gitwide.ampverse","cloud.gitwide.ampverse.MainActivity"))));}}
    @Override public void onStop(){active=false;if(connection!=null)MediaController.releaseFuture(connection);connection=null;controller=null;ipc(false,"disconnected while app is in background; server session unchanged");super.onStop();}
    @Override public void onDestroy(){closed=true;if(feature!=null)feature.close();handler.removeCallbacksAndMessages(null);worker.shutdownNow();super.onDestroy();}
}
