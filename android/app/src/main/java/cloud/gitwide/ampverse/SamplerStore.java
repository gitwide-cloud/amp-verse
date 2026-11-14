package cloud.gitwide.ampverse;
import android.content.*;
import android.content.pm.*;
import android.media.MediaMetadataRetriever;
import android.net.Uri;
import android.os.Binder;
import org.json.*;
import java.io.*;
import java.util.UUID;

/** One revisioned store for the base service's sampler; companion paths never escape IPC. */
final class SamplerStore {
    record Imported(File file,int side,int pad,String name,boolean loop,long expected){}
    static void indices(int side,int pad){if(side<0||side>1||pad<0||pad>8)throw new IllegalArgumentException("Choose sampler A/B and pad 1–9");}
    static String key(int side,int pad){indices(side,pad);return side+"-"+pad;}
    static long revision(SharedPreferences prefs,int side,int pad){return prefs.getLong("sample-revision-"+key(side,pad),0);}
    static void expect(SharedPreferences prefs,int side,int pad,long expected){if(revision(prefs,side,pad)!=expected)throw new IllegalStateException("This sampler pad changed while the clip was prepared. Choose the pad again.");}
    static void save(SharedPreferences prefs,int side,int pad,String path,String name,boolean loop){String k=key(side,pad);long revision=revision(prefs,side,pad);if(!prefs.edit().putString("sample-"+k,path).putString("name-"+k,name).putBoolean("loop-"+k,loop).putLong("sample-revision-"+k,Math.addExact(revision,1)).commit())throw new IllegalStateException("Sampler settings could not be saved");}
    static void clear(SharedPreferences prefs,int side,int pad){String k=key(side,pad);if(!prefs.edit().remove("sample-"+k).remove("name-"+k).putLong("sample-revision-"+k,Math.addExact(revision(prefs,side,pad),1)).commit())throw new IllegalStateException("Sampler settings could not be saved");}
    static JSONObject slots(SharedPreferences prefs)throws Exception{JSONArray sides=new JSONArray();for(int s=0;s<2;s++){JSONArray pads=new JSONArray();for(int p=0;p<9;p++){String k=key(s,p),path=prefs.getString("sample-"+k,"");pads.put(new JSONObject().put("pad",p).put("name",prefs.getString("name-"+k,"PAD "+(p+1))).put("loaded",!path.isEmpty()).put("loop",prefs.getBoolean("loop-"+k,false)).put("revision",revision(prefs,s,p)));}sides.put(pads);}return new JSONObject().put("sides",sides);}
    static boolean referenced(SharedPreferences prefs,File file){for(int s=0;s<2;s++)for(int p=0;p<9;p++)if(file.toString().equals(prefs.getString("sample-"+key(s,p),"")))return true;return false;}
    static Imported prepare(Context context,JSONObject args)throws Exception{
        int side=args.getInt("side"),pad=args.getInt("pad");indices(side,pad);long expected=args.getLong("revision");String name=args.getString("name").trim();if(name.isEmpty()||name.length()>160)throw new IOException("Enter a sample name up to 160 characters");
        Uri source=Uri.parse(args.getString("uri"));if(!"content".equals(source.getScheme()))throw new IOException("Export the clip from AMP Lab");
        ProviderInfo provider=context.getPackageManager().resolveContentProvider(source.getAuthority(),0);if(provider==null||context.getPackageManager().checkSignatures(provider.applicationInfo.uid,android.os.Process.myUid())!=PackageManager.SIGNATURE_MATCH)throw new SecurityException("The clip must come from a signed AMP Lab provider");
        File root=new File(context.getFilesDir(),"sampler");if(!root.isDirectory()&&!root.mkdirs())throw new IOException("Cannot create sampler storage");File output=new File(root,"import-"+UUID.randomUUID()+".mp3");long identity=Binder.clearCallingIdentity();
        try{try(InputStream in=context.getContentResolver().openInputStream(source);FileOutputStream out=new FileOutputStream(output)){if(in==null)throw new IOException("Cannot read the exported clip");byte[] buffer=new byte[65536];long count=0;int n;while((n=in.read(buffer))!=-1){if((count+=n)>8*1024*1024)throw new IOException("Sampler clip exceeds 8 MB");out.write(buffer,0,n);}out.getFD().sync();}
            MediaMetadataRetriever verify=new MediaMetadataRetriever();try{verify.setDataSource(output.toString());String length=verify.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION);if(length==null||Long.parseLong(length)<=0||Long.parseLong(length)>5100)throw new IOException("Sampler clips must be five seconds or shorter. Trim the clip in Lab.");}finally{verify.release();}
            return new Imported(output,side,pad,name,args.optBoolean("loop"),expected);
        }catch(Exception error){output.delete();throw error;}finally{Binder.restoreCallingIdentity(identity);}
    }
}
