package cloud.gitwide.ampverse;

import android.app.*;
import android.content.*;
import android.content.pm.*;
import android.net.Uri;
import android.os.*;
import android.widget.*;
import androidx.core.content.FileProvider;
import org.json.*;
import java.io.*;
import java.security.MessageDigest;
import java.util.*;

/** Signed Android applications, never downloaded code loaded into AMP Player. */
public final class Modules {
    private static final java.util.concurrent.atomic.AtomicBoolean downloading=new java.util.concurrent.atomic.AtomicBoolean();
    static final String BASE="cloud.gitwide.ampverse";
    static final String[] IDS={"dj","lab","cast","podcasts","remote"};
    static String label(String id){return id.equals("dj")?"AMP DJ":id.equals("lab")?"AMP Lab":id.equals("cast")?"AMP Cast":id.equals("remote")?"AMP Remote":"AMP Podcasts";}
    static String moduleId(String label){for(String id:IDS)if(label(id).equals(label))return id;return null;}
    static boolean embedded(String id){return BuildConfig.EDITION.equals("full")&&Arrays.asList(IDS).contains(id);}
    static PackageInfo installed(Context c,String id)throws Exception{
        if(!Arrays.asList(IDS).contains(id))throw new IOException("Unknown AMP module");
        PackageInfo info=c.getPackageManager().getPackageInfo(BASE+"."+id,PackageManager.GET_META_DATA|PackageManager.GET_SIGNING_CERTIFICATES);
        if(c.getPackageManager().checkSignatures(BASE,info.packageName)!=PackageManager.SIGNATURE_MATCH)throw new IOException("This module has a different signing identity");
        Bundle meta=info.applicationInfo.metaData;
        if(meta==null||meta.getInt("amp.api")!=1||!id.equals(meta.getString("amp.module"))||meta.getInt("amp.minCore")>BuildConfig.VERSION_CODE)throw new IOException("Update AMP Player and its module to compatible versions");
        return info;
    }
    public static void open(Activity a,String id){
        try{installed(a,id);if(!a.getSharedPreferences("modules",0).getBoolean("enabled-"+id,true))throw new IOException("This module is disabled in Add-ons");a.startActivity(new Intent().setClassName(BASE+"."+id,BASE+".CompanionActivity"));}
        catch(Exception error){new AlertDialog.Builder(a).setTitle(label(id)).setMessage("Install or enable this signed companion in Add-ons. "+error.getMessage()).setPositiveButton("Add-ons",(d,w)->show(a)).setNegativeButton("Close",null).show();}
    }
    static void show(Activity a){ModuleManager.show(a);}
    static void download(Activity activity,String id){
        if(AppUpdate.busy||!downloading.compareAndSet(false,true)){Toast.makeText(activity,"An update is already downloading. Wait or cancel it first.",Toast.LENGTH_LONG).show();return;}
        ProgressDialog dialog=new ProgressDialog(activity);dialog.setTitle(label(id));dialog.setMessage("Checking GitHub modules…");dialog.setProgressStyle(ProgressDialog.STYLE_HORIZONTAL);dialog.setMax(100);dialog.setCancelable(false);ModuleDownloads http=new ModuleDownloads();java.util.concurrent.atomic.AtomicBoolean cancelled=http.cancelled;dialog.setButton(DialogInterface.BUTTON_NEGATIVE,"Cancel",(d,w)->http.cancel());dialog.show();Context app=activity.getApplicationContext();
        new Thread(()->{File partial=null;try{
            JSONObject manifest=http.catalog();
            if(manifest.getInt("schema")!=1)throw new IOException("Unsupported module catalog");
            JSONObject entry=manifest.getJSONObject("android").getJSONObject(id);String pkg=entry.getString("package_id"),name=entry.getString("name"),hash=entry.getString("sha256");long size=entry.getLong("size");
            if(!pkg.equals(BASE+"."+id)||entry.getInt("api")!=1||entry.getInt("min_core")>BuildConfig.VERSION_CODE||entry.getInt("min_sdk")>Build.VERSION.SDK_INT||!entry.getJSONArray("abis").toString().contains("arm64-v8a"))throw new IOException("Update AMP Player first; this module is not compatible");
            if(!name.matches("[A-Za-z0-9][A-Za-z0-9._-]{0,178}\\.apk")||name.contains("..")||!hash.matches("[a-fA-F0-9]{64}")||size<1||size>536870912)throw new IOException("Invalid module download information");
            File directory=new File(app.getFilesDir(),"updates");if(!directory.isDirectory()&&!directory.mkdirs())throw new IOException("Cannot create download folder");partial=new File(directory,name+".part");
            MessageDigest digest=MessageDigest.getInstance("SHA-256");long count=0;
            try(InputStream in=http.open(entry.getString("url"));FileOutputStream out=new FileOutputStream(partial)){byte[] buffer=new byte[65536];int n;while((n=in.read(buffer))!=-1){if(cancelled.get())throw new IOException("Download cancelled");count+=n;if(count>size)throw new IOException("Module size mismatch");out.write(buffer,0,n);digest.update(buffer,0,n);final int progress=(int)(100*count/size);activity.runOnUiThread(()->dialog.setProgress(progress));}out.getFD().sync();}
            if(count!=size||!AppUpdate.hex(digest.digest()).equalsIgnoreCase(hash))throw new IOException("Module checksum mismatch");
            validate(app,partial,pkg,entry.getLong("version_code"),entry.getString("signer_sha256"));
            if(cancelled.get())throw new IOException("Download cancelled");File complete=new File(directory,name);java.nio.file.Files.move(partial.toPath(),complete.toPath(),java.nio.file.StandardCopyOption.REPLACE_EXISTING);
            app.getSharedPreferences("modules",0).edit().putString("pending",complete.toString()).putString("package",pkg).putLong("version",entry.getLong("version_code")).putString("signer",entry.getString("signer_sha256")).apply();activity.runOnUiThread(()->installPending(activity));
        }catch(Exception error){activity.runOnUiThread(()->{if(!activity.isDestroyed()&&!activity.isFinishing())new AlertDialog.Builder(activity).setTitle("Module download").setMessage(error.getMessage()+". Your installed apps and music were kept.").setPositiveButton("Retry",(d,w)->download(activity,id)).setNegativeButton("Close",null).show();});}
        finally{downloading.set(false);if(partial!=null)partial.delete();activity.runOnUiThread(()->{if(!activity.isDestroyed())dialog.dismiss();});}},"AMP module download").start();
    }
    static void validate(Context c,File apk,String pkg,long version,String expectedSigner)throws Exception{
        PackageManager pm=c.getPackageManager();PackageInfo incoming=pm.getPackageArchiveInfo(apk.toString(),PackageManager.GET_SIGNING_CERTIFICATES|PackageManager.GET_META_DATA),base=pm.getPackageInfo(BASE,PackageManager.GET_SIGNING_CERTIFICATES);
        if(incoming==null||!pkg.equals(incoming.packageName)||incoming.getLongVersionCode()!=version||incoming.signingInfo==null||base.signingInfo==null)throw new IOException("Invalid AMP companion APK");
        Set<String> a=new HashSet<>(),b=new HashSet<>();for(Signature s:incoming.signingInfo.getApkContentsSigners())a.add(AppUpdate.hex(MessageDigest.getInstance("SHA-256").digest(s.toByteArray())));for(Signature s:base.signingInfo.getApkContentsSigners())b.add(AppUpdate.hex(MessageDigest.getInstance("SHA-256").digest(s.toByteArray())));
        if(a.size()!=1||!a.equals(b)||!a.contains(expectedSigner.toLowerCase(Locale.ROOT)))throw new IOException("Module signing identity differs from AMP Player");
        Bundle meta=incoming.applicationInfo.metaData;if(meta==null||meta.getInt("amp.api")!=1||meta.getInt("amp.minCore")>BuildConfig.VERSION_CODE||!pkg.equals(BASE+"."+meta.getString("amp.module")))throw new IOException("Incompatible module interface");
        try{if(pm.getPackageInfo(pkg,0).getLongVersionCode()>version)throw new IOException("A newer module is already installed");}catch(PackageManager.NameNotFoundException ignored){}
    }
    static void installPending(Activity activity){if(activity.isDestroyed()||activity.isFinishing())return;SharedPreferences prefs=activity.getSharedPreferences("modules",0);String path=prefs.getString("pending","");if(path.isEmpty())return;try{File file=new File(path),root=new File(activity.getFilesDir(),"updates");if(!file.getCanonicalPath().startsWith(root.getCanonicalPath()+File.separator))throw new IOException("Invalid module download path");validate(activity,file,prefs.getString("package",""),prefs.getLong("version",0),prefs.getString("signer",""));
        if(!activity.getPackageManager().canRequestPackageInstalls()){if(!prefs.getBoolean("permission_prompted",false)){prefs.edit().putBoolean("permission_prompted",true).apply();activity.startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,Uri.parse("package:"+activity.getPackageName())));}return;}
        prefs.edit().remove("pending").remove("permission_prompted").apply();Uri uri=FileProvider.getUriForFile(activity,activity.getPackageName()+".updates",file);activity.startActivity(new Intent(Intent.ACTION_VIEW).setDataAndType(uri,"application/vnd.android.package-archive").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION));
    }catch(Exception error){prefs.edit().remove("pending").apply();Toast.makeText(activity,error.getMessage(),Toast.LENGTH_LONG).show();}}
}
