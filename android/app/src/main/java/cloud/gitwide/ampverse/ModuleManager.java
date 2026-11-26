package cloud.gitwide.ampverse;
import android.app.*;
import android.content.*;
import android.content.pm.PackageInfo;
import android.net.Uri;
import android.os.Build;
import android.widget.*;
import org.json.*;

/** A small native catalog, with explicit installed and compatible-update views. */
final class ModuleManager extends LinearLayout {
    final Activity a;final Appearance colors;final LinearLayout rows;
    final TextView status;final Button[] tabs=new Button[3];
    JSONObject catalog;int tab;boolean fetching;
    ModuleManager(Activity a){super(a);this.a=a;colors=new Appearance(a);setOrientation(VERTICAL);setPadding(dp(10),dp(8),dp(10),dp(8));setBackground(Chrome.panel(colors,false));
        addView(label("AMP Verse "+BuildConfig.EDITION+" · "+BuildConfig.VERSION_NAME,17));LinearLayout nav=new LinearLayout(a);String[] names={"Discover","Installed","Updates"};for(int i=0;i<3;i++){final int n=i;tabs[i]=button(names[i],()->{tab=n;draw();});nav.addView(tabs[i],new LayoutParams(0,dp(50),1));}addView(nav);
        status=label("Checking official GitHub packages…",13);addView(status);rows=new LinearLayout(a);rows.setOrientation(VERTICAL);addView(rows);draw();fetch();
    }
    int dp(int n){return (int)(n*getResources().getDisplayMetrics().density);}
    TextView label(String title,int size){TextView v=new TextView(a);v.setText(title);v.setTextColor(colors.text);v.setTextSize(size);v.setPadding(dp(4),dp(5),dp(4),dp(5));return v;}
    Button button(String title,Runnable action){Button b=new Button(a);b.setText(title);b.setAllCaps(false);b.setTextColor(colors.text);b.setMinHeight(dp(48));b.setBackground(Chrome.button(colors,false));b.setOnClickListener(v->action.run());return b;}
    static String description(String id){return switch(id){case "dj"->"Two-deck mixing, prepared playlists, Cue Decks and sampler.";case "lab"->"Trim, fades, gain, normalize and MP3 export. YouTube imports live here.";case "cast"->"Find stations, listen, save playlists with cues and use Google Cast.";case "remote"->"Discover and control your desktop Player or DJ.";default->"Browse public podcasts and RSS feeds; preview, subscribe and save episodes.";};}
    void fetch(){if(fetching)return;fetching=true;new Thread(()->{try{JSONObject result=new ModuleDownloads().catalog();if(result.getInt("schema")!=1)throw new IllegalStateException("Unsupported module catalog");a.runOnUiThread(()->{fetching=false;catalog=result;status.setText("Official GitHub catalog · "+result.optString("version"));draw();});}catch(Exception error){a.runOnUiThread(()->{fetching=false;status.setText(error.getMessage()+". Installed modules still work offline.");draw();});}},"AMP module catalog").start();}
    String incompatibility(JSONObject entry){if(entry==null)return "No downloadable package is published in this catalog.";if(entry.optInt("api")!=1||entry.optInt("min_core",Integer.MAX_VALUE)>BuildConfig.VERSION_CODE)return "Update AMP Verse before installing this module.";if(entry.optInt("min_sdk",Integer.MAX_VALUE)>Build.VERSION.SDK_INT)return "This package needs a newer Android version.";return "";}
    void draw(){rows.removeAllViews();for(int i=0;i<3;i++){tabs[i].setBackground(Chrome.button(colors,tab==i));tabs[i].setTextColor(Chrome.ink(colors,tab==i));}
        int count=0;for(String id:Modules.IDS){PackageInfo installed=null;try{installed=Modules.installed(a,id);}catch(Exception ignored){}boolean included=Modules.embedded(id);JSONObject entries=catalog==null?null:catalog.optJSONObject("android"),entry=entries==null?null:entries.optJSONObject(id);boolean update=installed!=null&&entry!=null&&entry.optLong("version_code")>installed.getLongVersionCode()&&incompatibility(entry).isEmpty();
            if(tab==1&&!included&&installed==null||tab==2&&(!update||included))continue;count++;
            LinearLayout card=new LinearLayout(a);card.setOrientation(VERTICAL);card.setPadding(dp(5),dp(8),dp(5),dp(8));card.setBackground(Chrome.panel(colors,false));rows.addView(card);
            final PackageInfo present=installed;String state=included?"Included in Full":present==null?"Not installed":"Installed · "+present.versionName;card.addView(button("ϟ "+Modules.label(id)+" · "+state,()->details(id,entry,present,included)));card.addView(label(description(id),14));
            if(entry!=null)card.addView(label("Download "+entry.optString("version")+" · "+String.format(java.util.Locale.ROOT,"%.1f MB",entry.optLong("size")/1048576.)+" · core "+entry.optInt("min_core")+"+",12));
        }
        if(count==0)rows.addView(label(tab==2?"No compatible companion updates are available. Included Full modules update with AMP Verse.":"No companion modules are installed yet.",15));
        rows.addView(button("Refresh GitHub catalog",this::fetch));
        if(!a.getSharedPreferences("modules",0).getString("pending","").isEmpty())rows.addView(button("Install downloaded module",()->{a.getSharedPreferences("modules",0).edit().remove("permission_prompted").apply();Modules.installPending(a);}));
        rows.addView(button("GitHub releases & source",()->a.startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse(AppUpdate.GITHUB+"/releases")))));
    }
    void details(String id,JSONObject entry,PackageInfo installed,boolean included){String text=description(id)+"\n\n"+(included?"Bundled with Full; no companion is required.":"Signed companion. AMP Player owns playback and library access.");if(entry!=null)text+="\n\nVersion "+entry.optString("version")+"\nDownload "+String.format(java.util.Locale.ROOT,"%.1f MB",entry.optLong("size")/1048576.)+"\nMinimum core: "+entry.optInt("min_core")+"\n\n"+entry.optString("release_notes");String reason=incompatibility(entry);if(!reason.isEmpty()&&!included)text+="\n\n"+reason;
        AlertDialog.Builder dialog=new AlertDialog.Builder(a).setTitle(Modules.label(id)).setMessage(text).setNegativeButton("Close",null);
        if(included)dialog.setPositiveButton("Open",(d,w)->a.startActivity(new Intent().setClassName(Modules.BASE,Modules.BASE+".MainActivity").addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP|Intent.FLAG_ACTIVITY_SINGLE_TOP).putExtra("module",Modules.label(id))));
        else if(installed!=null)dialog.setPositiveButton("Open",(d,w)->Modules.open(a,id));
        else if(reason.isEmpty())dialog.setPositiveButton("Install",(d,w)->Modules.download(a,id));
        if(installed!=null)dialog.setNeutralButton("Manage companion",(d,w)->new AlertDialog.Builder(a).setTitle(Modules.label(id)).setItems(new String[]{"Update from GitHub","Enable / disable","Uninstall companion"},(menu,n)->{if(n==0)Modules.download(a,id);else if(n==1){SharedPreferences prefs=a.getSharedPreferences("modules",0);boolean enabled=!prefs.getBoolean("enabled-"+id,true);prefs.edit().putBoolean("enabled-"+id,enabled).apply();Toast.makeText(a,enabled?"Enabled":"Disabled",Toast.LENGTH_SHORT).show();}else a.startActivity(new Intent(Intent.ACTION_DELETE,Uri.parse("package:"+Modules.BASE+"."+id)));}).setNegativeButton("Close",null).show());
        dialog.show();
    }
    static void show(Activity a){ScrollView scroll=new ScrollView(a);scroll.addView(new ModuleManager(a));new AlertDialog.Builder(a).setTitle("AMP Add-ons").setView(scroll).setPositiveButton("Done",null).show();}
}
