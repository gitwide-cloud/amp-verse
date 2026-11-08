package cloud.gitwide.ampverse;

import android.app.Activity;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.Uri;
import android.os.Bundle;
import android.provider.Settings;
import android.widget.*;
import java.util.concurrent.Executors;

/** App-wide settings; updates are checked on demand, installed by the user from Releases. */
public final class SettingsActivity extends Activity {
    static final String RELEASES = "https://github.com/gitwide-cloud/amp-verse/releases";
    private SharedPreferences prefs;
    private Appearance colors;
    private LinearLayout page;
    private boolean checking;
    private String updateStatus = "Installed: " + BuildConfig.VERSION_NAME + " · Check GitHub for a published update.";
    @Override public void onCreate(Bundle state) { super.onCreate(state); prefs=getSharedPreferences("settings",MODE_PRIVATE); draw(); }
    private int dp(int n) { return (int)(n*getResources().getDisplayMetrics().density); }
    private void label(String text, int size, int color) { TextView view=new TextView(this);view.setText(text);view.setTextSize(size);view.setTextColor(color);view.setPadding(0,dp(12),0,dp(8));page.addView(view); }
    private void button(String text, Runnable action) { Button view=new Button(this);view.setText(text);view.setAllCaps(false);view.setTextColor(colors.text);view.setBackgroundTintList(android.content.res.ColorStateList.valueOf(colors.surface));view.setOnClickListener(v->action.run());page.addView(view); }
    private void open(String url) { try { startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url))); } catch (android.content.ActivityNotFoundException error) { Toast.makeText(this,"No browser is available",Toast.LENGTH_LONG).show(); } }
    private void draw() {
        colors=new Appearance(this);getWindow().setStatusBarColor(colors.background);getWindow().setNavigationBarColor(colors.background);
        ScrollView scroll=new ScrollView(this);scroll.setBackgroundColor(colors.background);
        page=new LinearLayout(this);page.setOrientation(LinearLayout.VERTICAL);page.setPadding(dp(18),dp(8),dp(18),dp(24));scroll.addView(page);
        scroll.setOnApplyWindowInsetsListener((view,insets)->{view.setPadding(insets.getSystemWindowInsetLeft(),insets.getSystemWindowInsetTop(),insets.getSystemWindowInsetRight(),insets.getSystemWindowInsetBottom());return insets;});setContentView(scroll);
        button("‹ AMP VERSE",this::finish);label("Settings",28,colors.accent);
        label("Appearance",20,colors.text);
        RadioGroup themes=new RadioGroup(this);String[] names={"Classic","Modern Light","Modern Dark"},ids={"classic","modern-light","modern-dark"};
        for(int i=0;i<names.length;i++){final String id=ids[i];RadioButton radio=new RadioButton(this);radio.setId(android.view.View.generateViewId());radio.setText(names[i]);radio.setTag(id);radio.setTextColor(colors.text);radio.setButtonTintList(new android.content.res.ColorStateList(new int[][]{new int[]{android.R.attr.state_checked},new int[]{}},new int[]{colors.accent,colors.muted}));themes.addView(radio);radio.setChecked(prefs.getString("theme","classic").equals(id));radio.setOnClickListener(v->{prefs.edit().putString("theme",id).apply();draw();});}page.addView(themes);
        label("Playback",20,colors.text);
        CheckBox repeat=new CheckBox(this);repeat.setText("Repeat queue");repeat.setTag("repeat");repeat.setTextColor(colors.text);repeat.setButtonTintList(new android.content.res.ColorStateList(new int[][]{new int[]{android.R.attr.state_checked},new int[]{}},new int[]{colors.accent,colors.muted}));repeat.setChecked(prefs.getBoolean("repeat",false));repeat.setOnCheckedChangeListener((v,checked)->prefs.edit().putBoolean("repeat",checked).apply());page.addView(repeat);
        label("Sequential playback uses locally saved songs. Changes apply when you return to the player.",14,colors.muted);
        button("Android notification settings",()->startActivity(new Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE,getPackageName())));
        label("Local storage",20,colors.text);label("Imports are copied to this device. Removing a queue entry keeps its local copy; originals are untouched.",14,colors.muted);
        label("App updater",20,colors.text);label(updateStatus,14,colors.muted);
        button(checking?"Checking…":"Check for updates",()->{if(!checking)checkUpdates();});
        button("Open GitHub Releases / Download",()->open(RELEASES));
        label("Downloads open in your browser. Android asks you to approve installation. An update must use the same app signing key. Nothing is downloaded or installed automatically.",14,colors.muted);
        label("About",20,colors.text);label("AMP Verse "+BuildConfig.VERSION_NAME+" · Integrated Android app · No plugin loader",14,colors.muted);
    }
    private void checkUpdates(){
        checking=true;updateStatus="Checking GitHub Releases…";draw();
        java.util.concurrent.ExecutorService worker=Executors.newSingleThreadExecutor();worker.execute(()->{
            String result=ReleaseCheck.check(BuildConfig.VERSION_NAME);
            runOnUiThread(()->{checking=false;updateStatus=result;if(!isFinishing()&&!isDestroyed())draw();});worker.shutdown();
        });
    }
}
