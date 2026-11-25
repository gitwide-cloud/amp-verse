package cloud.gitwide.ampverse;

import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;
import androidx.media3.common.MediaItem;
import java.io.IOException;

/** Only a user-selected local audio URI; never claim arbitrary links or import files. */
final class ExternalAudio {
    static final String GRANT_ACTION="cloud.gitwide.ampverse.GRANT_EXTERNAL_AUDIO";
    static boolean accepts(Intent intent) {
        if(intent==null||!Intent.ACTION_VIEW.equals(intent.getAction())||intent.getData()==null)return false;
        String scheme=intent.getData().getScheme(),type=intent.getType();
        return ("content".equals(scheme)||"file".equals(scheme))&&type!=null&&
            (type.startsWith("audio/")||type.equals("application/ogg")||type.equals("application/x-ogg")||type.equals("application/flac")||type.equals("application/x-flac"));
    }
    static MediaItem prepare(Context context,Intent request)throws IOException {
        if(!accepts(request))throw new IOException("Unsupported audio request");
        Uri uri=request.getData();
        try(android.content.res.AssetFileDescriptor file=context.getContentResolver().openAssetFileDescriptor(uri,"r")){
            if(file==null)throw new IOException("Audio file unavailable");
        }
        String name=uri.getLastPathSegment();if(name==null||name.isEmpty())name="Audio file";
        if("content".equals(uri.getScheme()))try(Cursor cursor=context.getContentResolver().query(uri,new String[]{OpenableColumns.DISPLAY_NAME},null,null,null)){
            if(cursor!=null&&cursor.moveToFirst()&&!cursor.isNull(0))name=cursor.getString(0);
        }catch(RuntimeException ignored){} // Providers need not supply a display name.
        return LibraryStore.reference(context,uri,name,null).buildUpon().setMediaId("external-audio").build();
    }
}
