package cloud.gitwide.ampverse;

import android.content.ContentResolver;
import android.database.Cursor;
import android.net.Uri;
import android.provider.DocumentsContract;
import java.io.IOException;
import java.util.*;

/** Iterative SAF tree traversal: chosen folder only, no storage-wide permission. */
final class FolderScanner {
    private static final ThreadLocal<Map<String,String>> paths = ThreadLocal.withInitial(HashMap::new);
    private static Uri child(ContentResolver resolver, Uri parent, String name)throws IOException {
        String id=DocumentsContract.getDocumentId(parent);
        try(Cursor c=resolver.query(DocumentsContract.buildChildDocumentsUriUsingTree(parent,id),new String[]{DocumentsContract.Document.COLUMN_DOCUMENT_ID,DocumentsContract.Document.COLUMN_DISPLAY_NAME},null,null,null)) {
            if(c!=null)while(c.moveToNext())if(name.equals(c.getString(c.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME))))return DocumentsContract.buildDocumentUriUsingTree(parent,c.getString(c.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)));
        } return null;
    }
    static String preparation(ContentResolver resolver,Uri tree,Uri audio)throws Exception {
        String relative=paths.get().get(audio.toString()); if(relative==null)return null;
        Uri current=DocumentsContract.buildDocumentUriUsingTree(tree,DocumentsContract.getTreeDocumentId(tree));
        current=child(resolver,current,"ampdj"); if(current==null)return null;
        current=child(resolver,current,"tracks"); if(current==null)return null;
        for(String component:(relative+".json").split("/")){if(component.isEmpty()||component.equals(".."))return null;current=child(resolver,current,component);if(current==null)return null;}
        try(java.io.InputStream input=resolver.openInputStream(current)) {
            if(input==null)return null; java.io.ByteArrayOutputStream bytes=new java.io.ByteArrayOutputStream();byte[] buffer=new byte[8192];int count;
            while((count=input.read(buffer))!=-1){if(bytes.size()+count>4194304)throw new IOException("Preparation file too large");bytes.write(buffer,0,count);}
            String text=bytes.toString("UTF-8");org.json.JSONObject prep=new org.json.JSONObject(text);
            double start=prep.optDouble("cue_in",0);if(!Double.isFinite(start)||start<0)throw new IOException("Invalid saved cue");return text;
        }
    }

    static boolean audio(String name,String mime) {
        if(mime!=null&&mime.startsWith("audio/"))return true;
        return name!=null&&name.toLowerCase(Locale.ROOT).matches(".*\\.(mp3|wav|flac|ogg|opus|m4a|aac|aif|aiff|amr|wma)");
    }
    static List<Uri> scan(ContentResolver resolver,Uri tree)throws IOException {
        List<Uri> songs=new ArrayList<>();ArrayDeque<String[]> directories=new ArrayDeque<>();Set<String> visited=new HashSet<>();
        paths.get().clear();directories.add(new String[]{DocumentsContract.getTreeDocumentId(tree),""});
        while(!directories.isEmpty()){
            String[] next=directories.removeFirst();String directory=next[0];if(!visited.add(directory))continue;
            Uri children=DocumentsContract.buildChildDocumentsUriUsingTree(tree,directory);
            try(Cursor cursor=resolver.query(children,new String[]{DocumentsContract.Document.COLUMN_DOCUMENT_ID,DocumentsContract.Document.COLUMN_MIME_TYPE,DocumentsContract.Document.COLUMN_DISPLAY_NAME},null,null,null)){
                if(cursor==null)throw new IOException("The selected folder is unavailable");
                List<String[]> entries=new ArrayList<>();while(cursor.moveToNext())entries.add(new String[]{cursor.getString(0),cursor.getString(1),cursor.getString(2)});
                entries.sort(Comparator.comparing(entry->entry[2]==null?"":entry[2],String.CASE_INSENSITIVE_ORDER));
                for(String[] entry:entries){if(DocumentsContract.Document.MIME_TYPE_DIR.equals(entry[1])){if(!"ampdj".equals(entry[2])&&!".ampverse".equals(entry[2]))directories.addLast(new String[]{entry[0],next[1]+entry[2]+"/"});}else if(audio(entry[2],entry[1])){Uri song=DocumentsContract.buildDocumentUriUsingTree(tree,entry[0]);songs.add(song);paths.get().put(song.toString(),next[1]+entry[2]);}}
            }catch(SecurityException error){throw new IOException("Folder access was denied. Choose the folder again.",error);}
        }
        return songs;
    }
}
