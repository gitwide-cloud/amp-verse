package cloud.gitwide.ampverse;

import android.content.ContentResolver;
import android.database.Cursor;
import android.net.Uri;
import android.provider.DocumentsContract;
import java.io.IOException;
import java.util.*;

/** Iterative SAF tree traversal: chosen folder only, no storage-wide permission. */
final class FolderScanner {
    static boolean audio(String name,String mime) {
        if(mime!=null&&mime.startsWith("audio/"))return true;
        return name!=null&&name.toLowerCase(Locale.ROOT).matches(".*\\.(mp3|wav|flac|ogg|opus|m4a|aac|aif|aiff|amr|wma)");
    }
    static List<Uri> scan(ContentResolver resolver,Uri tree)throws IOException {
        List<Uri> songs=new ArrayList<>();ArrayDeque<String> directories=new ArrayDeque<>();Set<String> visited=new HashSet<>();
        directories.add(DocumentsContract.getTreeDocumentId(tree));
        while(!directories.isEmpty()){
            String directory=directories.removeFirst();if(!visited.add(directory))continue;
            Uri children=DocumentsContract.buildChildDocumentsUriUsingTree(tree,directory);
            try(Cursor cursor=resolver.query(children,new String[]{DocumentsContract.Document.COLUMN_DOCUMENT_ID,DocumentsContract.Document.COLUMN_MIME_TYPE,DocumentsContract.Document.COLUMN_DISPLAY_NAME},null,null,null)){
                if(cursor==null)throw new IOException("The selected folder is unavailable");
                List<String[]> entries=new ArrayList<>();while(cursor.moveToNext())entries.add(new String[]{cursor.getString(0),cursor.getString(1),cursor.getString(2)});
                entries.sort(Comparator.comparing(entry->entry[2]==null?"":entry[2],String.CASE_INSENSITIVE_ORDER));
                for(String[] entry:entries){if(DocumentsContract.Document.MIME_TYPE_DIR.equals(entry[1]))directories.addLast(entry[0]);else if(audio(entry[2],entry[1]))songs.add(DocumentsContract.buildDocumentUriUsingTree(tree,entry[0]));}
            }catch(SecurityException error){throw new IOException("Folder access was denied. Choose the folder again.",error);}
        }
        return songs;
    }
}
