package cloud.gitwide.ampverse;

import android.content.ContentResolver;
import android.database.Cursor;
import android.net.Uri;
import android.provider.DocumentsContract;
import java.io.IOException;
import java.util.*;

/** Iterative SAF tree traversal: chosen folder only, no storage-wide permission. */
final class FolderScanner {
    private static final ThreadLocal<Map<String,String>> packed = ThreadLocal.withInitial(HashMap::new);
    private static final ThreadLocal<Map<String,String>> paths = ThreadLocal.withInitial(HashMap::new);
    private static Uri child(ContentResolver resolver, Uri parent, String name)throws IOException {
        String id=DocumentsContract.getDocumentId(parent);
        try(Cursor c=resolver.query(DocumentsContract.buildChildDocumentsUriUsingTree(parent,id),new String[]{DocumentsContract.Document.COLUMN_DOCUMENT_ID,DocumentsContract.Document.COLUMN_DISPLAY_NAME},null,null,null)) {
            if(c!=null)while(c.moveToNext())if(name.equals(c.getString(c.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME))))return DocumentsContract.buildDocumentUriUsingTree(parent,c.getString(c.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)));
        } return null;
    }
    static String preparation(ContentResolver resolver,Uri tree,Uri audio)throws Exception {
        String relative=paths.get().get(audio.toString()); if(relative==null)return null;
        String value=packed.get().get("tracks/"+relative+".json");if(value!=null)return value;
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

    static void catalog(android.content.Context context,Uri tree)throws Exception {
        packed.get().clear();
        ContentResolver resolver=context.getContentResolver();
        Uri root=DocumentsContract.buildDocumentUriUsingTree(tree,DocumentsContract.getTreeDocumentId(tree));
        Uri folder=child(resolver,root,"ampdj");if(folder==null)return;
        Uri catalog=child(resolver,folder,"catalog.ampdb");if(catalog==null)return;
        java.io.File copy=copyCatalog(context,resolver,catalog);
        try(android.database.sqlite.SQLiteDatabase db=android.database.sqlite.SQLiteDatabase.openDatabase(copy.getPath(),null,android.database.sqlite.SQLiteDatabase.OPEN_READONLY)) {
            int version=db.getVersion();
            if(version==1)loadRecords(db);
            else if(version==2){
                Uri snapshots=child(resolver,folder,"catalogs");if(snapshots==null)throw new IOException("Song catalog folder is missing");
                try(Cursor c=db.rawQuery("SELECT file FROM shards ORDER BY id",null)){
                    while(c.moveToNext()){
                        String name=c.getString(0);if(name==null||!name.matches("songs-[A-Za-z0-9-]+\\.ampdb"))throw new IOException("Invalid song catalog location");
                        Uri source=child(resolver,snapshots,name);if(source==null)throw new IOException("A song catalog is unavailable");
                        java.io.File shard=copyCatalog(context,resolver,source);
                        try(android.database.sqlite.SQLiteDatabase songs=android.database.sqlite.SQLiteDatabase.openDatabase(shard.getPath(),null,android.database.sqlite.SQLiteDatabase.OPEN_READONLY)){if(songs.getVersion()!=1)throw new IOException("Unsupported song catalog version");loadRecords(songs);}finally{shard.delete();}
                    }
                }
            }else throw new IOException("Unsupported library catalog version");
        }catch(Exception error){packed.get().clear();throw error;}finally{copy.delete();}
    }
    private static java.io.File copyCatalog(android.content.Context context,ContentResolver resolver,Uri source)throws IOException {
        java.io.File copy=java.io.File.createTempFile("amp-catalog-",".ampdb",context.getCacheDir());boolean complete=false;
        try(java.io.InputStream input=resolver.openInputStream(source);java.io.OutputStream output=new java.io.FileOutputStream(copy)){
            if(input==null)throw new IOException("Cannot read catalog");byte[] buffer=new byte[65536];int count;long total=0;
            while((count=input.read(buffer))!=-1){total+=count;if(total>512L*1024*1024)throw new IOException("Catalog too large");output.write(buffer,0,count);}complete=true;return copy;
        }finally{if(!complete)copy.delete();}
    }
    private static void loadRecords(android.database.sqlite.SQLiteDatabase db)throws Exception {
        try(Cursor c=db.rawQuery("SELECT key,document FROM records WHERE key LIKE 'tracks/%'",null)){
            while(c.moveToNext()){String value=c.getString(1);org.json.JSONObject prep=new org.json.JSONObject(value);double cue=prep.optDouble("cue_in",0);if(!Double.isFinite(cue)||cue<0)throw new IOException("Invalid catalog cue");packed.get().put(c.getString(0),value);}
        }
    }
    static boolean audio(String name,String mime) {
        if(mime!=null&&mime.startsWith("audio/"))return true;
        return name!=null&&name.toLowerCase(Locale.ROOT).matches(".*\\.(mp3|wav|flac|ogg|opus|m4a|aac|aif|aiff|amr|wma)");
    }
    static List<Uri> scan(ContentResolver resolver,Uri tree)throws IOException {
        List<Uri> songs=new ArrayList<>();ArrayDeque<String[]> directories=new ArrayDeque<>();Set<String> visited=new HashSet<>();
        paths.get().clear();packed.get().clear();directories.add(new String[]{DocumentsContract.getTreeDocumentId(tree),""});
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
