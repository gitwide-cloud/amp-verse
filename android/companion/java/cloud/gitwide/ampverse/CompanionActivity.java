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
    boolean closed,active;
    int dp(int value){return (int)(value*getResources().getDisplayMetrics().density);}
    TextView label(String text,int size){TextView v=new TextView(this);v.setText(text);v.setTextSize(size);v.setTextColor(colors.text);v.setPadding(dp(6),dp(6),dp(6),dp(6));return v;}
    Button button(String title,Runnable action){Button b=new Button(this);b.setText(title);b.setAllCaps(false);b.setTextColor(colors.text);b.setMinHeight(dp(48));b.setBackground(Chrome.button(colors,false));b.setOnClickListener(v->action.run());return b;}
    LinearLayout column(){LinearLayout v=new LinearLayout(this);v.setOrientation(LinearLayout.VERTICAL);return v;}
    void row(LinearLayout parent,Button... buttons){LinearLayout row=new LinearLayout(this);for(Button b:buttons)row.addView(b,new LinearLayout.LayoutParams(0,dp(52),1));parent.addView(row);}
    interface Result {void accept(JSONObject value)throws Exception;}
    void rpc(String action,JSONObject data,Result result){rpc(action,data,result,()->{});}
    void rpc(String action,JSONObject data,Result result,Runnable complete){if(closed)return;worker.execute(()->{try{JSONObject response=ModuleRpc.call(this,action,data);runOnUiThread(()->{if(closed)return;try{result.accept(response);}catch(Exception e){status.setText(e.getMessage());}finally{complete.run();}});}catch(Exception error){runOnUiThread(()->{try{if(!closed)status.setText(error.getMessage());}finally{complete.run();}});}});}
    void command(String action,JSONObject data){rpc(action,data,result->status.setText(result.optString("message","Done")));}
    void work(Callable<String> action){worker.execute(()->{try{String message=action.call();runOnUiThread(()->{if(!closed)status.setText(message);});}catch(Exception e){runOnUiThread(()->{if(!closed)status.setText(e.getMessage());});}});}
    @Override public void onCreate(Bundle state){super.onCreate(state);if(BuildConfig.DEBUG&&getIntent().getBooleanExtra("amp-test-keep-awake",false))getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);colors=new Appearance(this);LinearLayout root=column();root.setBackground(Chrome.panel(colors,false));root.setPadding(dp(8),dp(8),dp(8),dp(8));root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(dp(8)+i.getSystemWindowInsetLeft(),i.getSystemWindowInsetTop(),dp(8)+i.getSystemWindowInsetRight(),i.getSystemWindowInsetBottom());return i;});
        row(root,button("ϟ AMP "+BuildConfig.MODULE.toUpperCase(java.util.Locale.ROOT),()->startActivity(new Intent().setClassName("cloud.gitwide.ampverse","cloud.gitwide.ampverse.MainActivity"))),button("Back to Player",()->{startActivity(new Intent().setClassName("cloud.gitwide.ampverse","cloud.gitwide.ampverse.MainActivity"));finish();}));
        status=label("Connecting to AMP Player…",14);root.addView(status);ScrollView scroll=new ScrollView(this);body=column();scroll.addView(body);root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));setContentView(root);
    }
    @Override public void onStart(){super.onStart();active=true;connectCore();}
    private void connectCore(){if(closed||!active||connection!=null)return;try{
        if(getPackageManager().checkSignatures(getPackageName(),"cloud.gitwide.ampverse")!=android.content.pm.PackageManager.SIGNATURE_MATCH)throw new IllegalStateException("Install AMP Player and this module from the same signed release.");
        connection=new MediaController.Builder(this,new SessionToken(this,new ComponentName("cloud.gitwide.ampverse","cloud.gitwide.ampverse.PlaybackService"))).setListener(new MediaController.Listener(){@Override public void onDisconnected(MediaController controller){if(connection!=null)MediaController.releaseFuture(connection);connection=null;if(active&&!closed)handler.postDelayed(CompanionActivity.this::connectCore,500);}}).buildAsync();
        ListenableFuture<MediaController> pending=connection;pending.addListener(()->{if(connection!=pending)return;try{controller=pending.get();rpc("hello",null,result->{if(result.optInt("api")!=1||result.optInt("core")<34)throw new IllegalStateException("Update AMP Player to use this module");if(feature==null){feature=new FeatureScreen(this);feature.open(body);}status.setText("Connected · phone playback is owned by AMP Player");});}catch(Exception error){status.setText("Cannot connect to AMP Player. Open or update it, then retry.");}},getMainExecutor());
    }catch(Exception error){status.setText(error.getMessage());body.addView(button("Open AMP Player",()->startActivity(new Intent().setClassName("cloud.gitwide.ampverse","cloud.gitwide.ampverse.MainActivity"))));}}
    @Override public void onStop(){active=false;if(connection!=null)MediaController.releaseFuture(connection);connection=null;controller=null;super.onStop();}
    @Override public void onDestroy(){closed=true;if(feature!=null)feature.close();handler.removeCallbacksAndMessages(null);worker.shutdownNow();super.onDestroy();}
}
