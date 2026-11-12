package cloud.gitwide.ampverse;
import android.app.Activity;
import android.content.*;
import android.content.pm.*;
import android.net.Uri;
import android.provider.Settings;
import androidx.core.content.FileProvider;
import org.json.*;
import java.io.*;
import java.net.*;
import java.security.MessageDigest;

/** Explicit official-release update. Android retains final installation approval. */
final class AppUpdate {
    static final String GITHUB="https://github.com/gitwide-cloud/amp-verse";
    static volatile boolean busy,cancelled;
    static volatile String status="Tap Update to download and install a newer release.";
    private static volatile HttpURLConnection active;
    private static String hex(byte[] bytes){StringBuilder s=new StringBuilder();for(byte b:bytes)s.append(String.format(java.util.Locale.ROOT,"%02x",b));return s.toString();}
    private static HttpURLConnection connection(String url)throws Exception {
        URL parsed=new URL(url);if(!parsed.getProtocol().equals("https"))throw new IOException("Update URL must use HTTPS");
        HttpURLConnection c=(HttpURLConnection)parsed.openConnection();c.setConnectTimeout(15000);c.setReadTimeout(5000);c.setRequestProperty("User-Agent","AMPVerse-Updater");c.setInstanceFollowRedirects(false);return c;
    }
    private static InputStream open(String url)throws Exception {
        for(int redirects=0;redirects<6;redirects++){
            if(cancelled)throw new IOException("Update cancelled");
            HttpURLConnection c=connection(url);active=c;int code=c.getResponseCode();
            if(code>=300&&code<400){String location=c.getHeaderField("Location");c.disconnect();URL next=new URL(new URL(url),location);String host=next.getHost();if(!next.getProtocol().equals("https")||!(host.equals("github.com")||host.endsWith(".githubusercontent.com")))throw new IOException("Untrusted update redirect");url=next.toString();continue;}
            if(code!=200){c.disconnect();throw new IOException(code==404?"No compatible public update has been published yet.":"GitHub update download failed (HTTP "+code+")");}
            return c.getInputStream();
        }throw new IOException("Too many download redirects");
    }
    private static JSONObject json(String url)throws Exception {
        try(InputStream in=open(url);ByteArrayOutputStream out=new ByteArrayOutputStream()){byte[] b=new byte[8192];int n;while((n=in.read(b))!=-1){if(out.size()+n>1048576)throw new IOException("Update metadata too large");out.write(b,0,n);}return new JSONObject(out.toString("UTF-8"));}finally{if(active!=null)active.disconnect();}
    }
    static void start(Context context){if(busy)return;Context app=context.getApplicationContext();busy=true;cancelled=false;status="Checking for updates…";
        new Thread(()->{File partial=null;try{
            JSONObject release=json("https://api.github.com/repos/gitwide-cloud/amp-verse/releases/latest");String tag=release.getString("tag_name");
            if(!tag.matches("v?[0-9]{1,8}\\.[0-9]{1,8}\\.[0-9]{1,8}")||release.optBoolean("draft")||release.optBoolean("prerelease"))throw new IOException("No stable update is published");
            if(ReleaseCheck.compare(tag,BuildConfig.VERSION_NAME)<=0){status="You have the latest Android update.";return;}
            String base=GITHUB+"/releases/download/"+tag+"/";JSONObject manifest=json(base+"amp-update.json");
            if(manifest.optInt("schema")!=1||ReleaseCheck.compare(manifest.getString("version"),tag)!=0)throw new IOException("Update manifest version mismatch");
            JSONObject p=manifest.getJSONObject("packages").getJSONObject("android-arm64");
            if(p.getLong("version_code")<=BuildConfig.VERSION_CODE){status="You have the latest Android update.";return;}
            String name=p.getString("name"),hash=p.getString("sha256");long size=p.getLong("size");
            if(!name.matches("[A-Za-z0-9][A-Za-z0-9._-]{0,178}\\.apk")||name.contains("..")||!hash.matches("[A-Fa-f0-9]{64}")||size<1||size>536870912)throw new IOException("Invalid update package information");
            File folder=new File(app.getFilesDir(),"updates");if(!folder.isDirectory()&&!folder.mkdirs())throw new IOException("Cannot create update download folder");
            File output=new File(folder,name);partial=new File(folder,name+".part");MessageDigest digest=MessageDigest.getInstance("SHA-256");long count=0;
            try(InputStream in=open(base+name);FileOutputStream out=new FileOutputStream(partial)){byte[] b=new byte[65536];int n;while((n=in.read(b))!=-1){if(cancelled)throw new IOException("Update cancelled");count+=n;if(count>size)throw new IOException("Update size mismatch");digest.update(b,0,n);out.write(b,0,n);status="Downloading update… "+(count*100/size)+"%";}out.getFD().sync();}
            if(count!=size||!hex(digest.digest()).equalsIgnoreCase(hash))throw new IOException("Update checksum mismatch; existing app was kept");
            validate(app,partial,p.getLong("version_code"));
            if(cancelled)throw new IOException("Update cancelled");
            if(output.exists()&&!output.delete())throw new IOException("Cannot replace cached update");
            if(!partial.renameTo(output))throw new IOException("Cannot save completed update");
            app.getSharedPreferences("updates",0).edit().putString("apk",output.getAbsolutePath()).putBoolean("install",true).apply();
            status="Download verified. Opening Android installer…";
        }catch(Exception error){status=cancelled?"Update cancelled; existing app was kept.":"Update failed: "+error.getMessage();}
        finally{if(active!=null){active.disconnect();active=null;}if(partial!=null&&partial.exists())partial.delete();busy=false;}
        },"AMP update").start();
    }
    static void cancel(){cancelled=true;HttpURLConnection c=active;if(c!=null)c.disconnect();}
    static void validate(Context c,File apk,long expected)throws Exception {
        PackageManager pm=c.getPackageManager();PackageInfo incoming=pm.getPackageArchiveInfo(apk.getAbsolutePath(),PackageManager.GET_SIGNING_CERTIFICATES);
        PackageInfo installed=pm.getPackageInfo(c.getPackageName(),PackageManager.GET_SIGNING_CERTIFICATES);
        if(incoming==null||!c.getPackageName().equals(incoming.packageName)||incoming.getLongVersionCode()!=expected||expected<=installed.getLongVersionCode())throw new IOException("APK is not a newer AMP Verse build");
        if(incoming.signingInfo==null||installed.signingInfo==null)throw new IOException("APK signing information missing");
        java.util.Set<String> a=new java.util.HashSet<>(),b=new java.util.HashSet<>();for(Signature s:incoming.signingInfo.getApkContentsSigners())a.add(hex(s.toByteArray()));for(Signature s:installed.signingInfo.getApkContentsSigners())b.add(hex(s.toByteArray()));
        if(a.isEmpty()||!a.equals(b))throw new IOException("APK signing key differs from this installation. Existing app was kept; use a release signed with the same key.");
    }
    static void installPending(Activity activity){
        if(busy)return;SharedPreferences prefs=activity.getSharedPreferences("updates",0);if(!prefs.getBoolean("install",false))return;
        if(cancelled){prefs.edit().putBoolean("install",false).apply();return;}
        try{File apk=new File(prefs.getString("apk",""));File directory=new File(activity.getFilesDir(),"updates");if(!apk.getCanonicalPath().startsWith(directory.getCanonicalPath()+File.separator)||!apk.isFile())throw new IOException("Downloaded update is missing; tap Update to retry");
            PackageInfo cached=activity.getPackageManager().getPackageArchiveInfo(apk.getAbsolutePath(),PackageManager.GET_SIGNING_CERTIFICATES);
            if(cached==null)throw new IOException("Downloaded APK is invalid; tap Update to retry");
            validate(activity,apk,cached.getLongVersionCode());
            if(!activity.getPackageManager().canRequestPackageInstalls()){status="Allow AMP Verse to install updates, then return here.";prefs.edit().putBoolean("install",false).putBoolean("permission_pending",true).apply();activity.startActivity(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,Uri.parse("package:"+activity.getPackageName())));return;}
            prefs.edit().putBoolean("install",false).putBoolean("permission_pending",false).apply();
            Uri uri=FileProvider.getUriForFile(activity,activity.getPackageName()+".updates",apk);
            activity.startActivity(new Intent(Intent.ACTION_VIEW).setDataAndType(uri,"application/vnd.android.package-archive").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION));status="Approve the Android installation. If cancelled, tap Install downloaded update to retry.";
        }catch(Exception error){prefs.edit().putBoolean("install",false).apply();status="Could not open installer: "+error.getMessage();}
    }
}
