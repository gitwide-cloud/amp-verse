package cloud.gitwide.ampverse;

import android.app.Activity;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.Uri;
import android.os.Bundle;
import android.provider.Settings;
import android.widget.*;

/** App-wide settings and verified in-app update installation. */
public final class SettingsActivity extends Activity {
    static final String RELEASES = "https://github.com/gitwide-cloud/amp-verse/releases";
    private SharedPreferences prefs;
    private Appearance colors;
    private LinearLayout page;
    private CheckBox mixerToggle;
    private TextView mixerHint;
    private Button samplerStop;
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
        if("full".equals(BuildConfig.EDITION)&&android.os.Build.VERSION.SDK_INT>=30){
            label("Audio",20,colors.text);
            mixerToggle=new CheckBox(this);mixerToggle.setText("New mixer (beta)");mixerToggle.setTag("new-mixer-beta");mixerToggle.setContentDescription("New mixer beta");mixerToggle.setTextColor(colors.text);mixerToggle.setButtonTintList(new android.content.res.ColorStateList(new int[][]{new int[]{android.R.attr.state_checked},new int[]{}},new int[]{colors.accent,colors.muted}));
            mixerToggle.setChecked(prefs.getBoolean("owned-pcm-master-candidate",false));
            mixerToggle.setOnCheckedChangeListener((view,checked)->{
                if(PlaybackService.mixerSettingsBusy()){Toast.makeText(this,"Stop playback and phone broadcasting before changing the mixer.",Toast.LENGTH_LONG).show();draw();return;}
                if(!prefs.edit().putBoolean("owned-pcm-master-candidate",checked).commit()){Toast.makeText(this,"Could not save the mixer setting.",Toast.LENGTH_LONG).show();draw();return;}
                Toast.makeText(this,"Saved. Close AMP Verse from Recents, then reopen it to apply the mixer change.",Toast.LENGTH_LONG).show();refreshMixerOption();
            });page.addView(mixerToggle);
            mixerHint=new TextView(this);mixerHint.setTextSize(14);mixerHint.setTextColor(colors.muted);page.addView(mixerHint);
            samplerStop=new Button(this);samplerStop.setText("Stop sampler pads");samplerStop.setAllCaps(false);samplerStop.setTag("mixer-stop-pads");samplerStop.setTextColor(colors.text);samplerStop.setBackgroundTintList(android.content.res.ColorStateList.valueOf(colors.surface));
            samplerStop.setOnClickListener(view->{PlaybackService.stopSamplerPads();Toast.makeText(this,"Sampler pads stopped.",Toast.LENGTH_SHORT).show();refreshMixerOption();});
            page.addView(samplerStop);refreshMixerOption();
        }
        label("Playback",20,colors.text);
        CheckBox repeat=new CheckBox(this);repeat.setText("Repeat queue");repeat.setTag("repeat");repeat.setTextColor(colors.text);repeat.setButtonTintList(new android.content.res.ColorStateList(new int[][]{new int[]{android.R.attr.state_checked},new int[]{}},new int[]{colors.accent,colors.muted}));repeat.setChecked(prefs.getBoolean("repeat",false));repeat.setOnCheckedChangeListener((v,checked)->prefs.edit().putBoolean("repeat",checked).apply());page.addView(repeat);
        label("Sequential playback uses locally saved songs. Changes apply when you return to the player.",14,colors.muted);
        button("Android notification settings",()->startActivity(new Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE,getPackageName())));
        label("Local storage",20,colors.text);label("Phone-folder music stays separate from station downloads. In AMP Cast, connect to a Smart Station and save its playlists with cues for offline listening. Offline cue changes stay local.",14,colors.muted);
        label("App updater",20,colors.text);label("Installed: "+BuildConfig.VERSION_NAME,14,colors.muted);
        button(AppUpdate.busy?"Updating…":"Update",()->{if(!AppUpdate.busy){AppUpdate.start(this);draw();}});
        button("GitHub",()->open(AppUpdate.GITHUB));
        if(AppUpdate.busy)button("Cancel update",AppUpdate::cancel);
        if(new java.io.File(getSharedPreferences("updates",0).getString("apk","/missing")).isFile())button("Install downloaded update",()->{AppUpdate.cancelled=false;getSharedPreferences("updates",0).edit().putBoolean("install",true).apply();AppUpdate.installPending(this);});
        label(AppUpdate.status,14,colors.muted);
        label("Update downloads the official APK, checks it, then opens Android’s installer. Approve the system installation prompt.",14,colors.muted);
        label("About",20,colors.text);label("AMP Verse "+BuildConfig.VERSION_NAME+" · Integrated Android app · No plugin loader",14,colors.muted);
    }
    private void refreshMixerOption(){
        if(mixerToggle==null)return;boolean busy=PlaybackService.mixerSettingsBusy();mixerToggle.setEnabled(!busy);mixerToggle.setAlpha(busy?.5f:1f);
        if(samplerStop!=null){boolean pads=PlaybackService.samplerPadsBusy();samplerStop.setEnabled(pads);samplerStop.setAlpha(pads?1f:.5f);}
        mixerHint.setText(busy?"Stop all playback, pads and phone broadcasting to change the mixer.":"Enables phone Player/DJ broadcasting on compatible outputs. Applies next time AMP Verse starts: close it from Recents while stopped, then reopen. Your saved music and playlists stay in place.");
    }
    private final android.os.Handler updateHandler=new android.os.Handler(android.os.Looper.getMainLooper());
    private String displayed="";
    private final Runnable updateTick=new Runnable(){public void run(){AppUpdate.installPending(SettingsActivity.this);refreshMixerOption();if(!displayed.equals(AppUpdate.status)){displayed=AppUpdate.status;draw();}updateHandler.postDelayed(this,300);}};
    @Override protected void onResume(){super.onResume();SharedPreferences u=getSharedPreferences("updates",0);if(u.getBoolean("permission_pending",false)&&getPackageManager().canRequestPackageInstalls())u.edit().putBoolean("permission_pending",false).putBoolean("install",true).apply();updateHandler.post(updateTick);}
    @Override protected void onPause(){updateHandler.removeCallbacks(updateTick);super.onPause();}
}
