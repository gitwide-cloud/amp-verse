package cloud.gitwide.ampverse;

import android.content.*;
import android.net.Uri;
import android.os.Build;
import android.provider.MediaStore;
import java.io.*;

/** Publishes only AMP-owned downloads. Never scans, moves or deletes the user's music. */
final class ManagedMusic {
    static final String PATH="Music/AMP Library/";
    static Uri stage(Context context,File audio,String playlist,String title)throws IOException {
        if(Build.VERSION.SDK_INT<29)throw new IOException("Music/AMP Library downloads require Android 10 or newer. Existing offline downloads remain available.");
        String folder=playlist.replaceAll("[^\\p{L}\\p{N} _-]","_").trim();if(folder.isEmpty())folder="Playlist";if(folder.length()>70)folder=folder.substring(0,70);
        String label=title.replaceAll("[^\\p{L}\\p{N} _-]","_").trim();if(label.length()>100)label=label.substring(0,100);
        ContentValues values=new ContentValues();values.put(MediaStore.Audio.Media.DISPLAY_NAME,(label.isEmpty()?"":label+" - ")+audio.getName());values.put(MediaStore.Audio.Media.TITLE,title);values.put(MediaStore.Audio.Media.RELATIVE_PATH,PATH+folder+"/");values.put(MediaStore.Audio.Media.IS_PENDING,1);
        String ext=audio.getName().substring(audio.getName().lastIndexOf('.')+1);String mime=android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext);values.put(MediaStore.Audio.Media.MIME_TYPE,mime==null?"audio/mpeg":mime);
        ContentResolver resolver=context.getContentResolver();Uri uri=resolver.insert(MediaStore.Audio.Media.EXTERNAL_CONTENT_URI,values);if(uri==null)throw new IOException("Cannot create Music/AMP Library download");
        try(InputStream in=new FileInputStream(audio);OutputStream out=resolver.openOutputStream(uri,"w")){if(out==null)throw new IOException("Cannot write downloaded song");byte[] buffer=new byte[65536];int n;while((n=in.read(buffer))!=-1){if(Thread.currentThread().isInterrupted())throw new IOException("Download cancelled");out.write(buffer,0,n);}return uri;}catch(Exception e){resolver.delete(uri,null,null);throw new IOException("Cannot save downloaded song: "+e.getMessage(),e);}
    }
    static void publish(Context context,Uri uri)throws IOException {ContentValues values=new ContentValues();values.put(MediaStore.Audio.Media.IS_PENDING,0);if(context.getContentResolver().update(uri,values,null,null)!=1)throw new IOException("Cannot finish downloaded song");}
    static void discard(Context context,Uri uri){try{context.getContentResolver().delete(uri,null,null);}catch(Exception ignored){}}
}
