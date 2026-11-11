package cloud.gitwide.ampverse;
import android.content.*;
import android.database.*;
import android.database.sqlite.SQLiteDatabase;
import java.io.*;
/** Embedded file database. No network, server, or analysis engine. */
final class CatalogStore {
    static void migrate(Context context){
        migrateFolder(context.getFilesDir(),false);
        File[] albums=new File(context.getFilesDir(),"radio-offline").listFiles();
        if(albums!=null)for(File album:albums)if(album.isDirectory()&&!album.getName().startsWith("."))migrateFolder(album,true);
    }
    static void migrateFolder(File folder,boolean offline){
        File db=new File(folder,"ampdj/catalog.ampdb");
        migrateFile(new File(folder,"ampdj/library.json"),db,"library");
        migrateFile(new File(folder,"library.json"),db,"library");
        migrateTracks(new File(folder,"ampdj/tracks"),new File(folder,"ampdj/tracks"),db);
        if(offline)migrateFile(new File(folder,"playlist.json"),db,"offline-playlist");
    }
    private static void migrateTracks(File dir,File root,File db){
        File[] files=dir.listFiles();if(files==null)return;
        for(File file:files) {if(file.isDirectory())migrateTracks(file,root,db);else if(file.getName().endsWith(".json"))migrateFile(file,db,"tracks/"+root.toPath().relativize(file.toPath()).toString().replace('\\','/'));}
    }
    private static void migrateFile(File source,File db,String key){
        if(!source.isFile())return;
        try {
            String document=new String(java.nio.file.Files.readAllBytes(source.toPath()),java.nio.charset.StandardCharsets.UTF_8);
            new org.json.JSONTokener(document).nextValue();
            String existing=get(db,key);
            String target=existing==null||existing.equals(document)?key:"recovery/"+key+"/"+java.util.UUID.randomUUID();
            put(db,target,document);
            if(!document.equals(get(db,target)))throw new IOException("Catalog verification failed");
            if(!source.delete())throw new IOException("Old metadata could not be retired");
        }catch(Exception error){throw new IllegalStateException("Metadata retained: "+source.getName()+": "+error.getMessage(),error);}
    }
    static File file(Context context){return new File(context.getFilesDir(),"ampdj/catalog.ampdb");}
    static String get(Context context,String key){return get(file(context),key);}
    static String get(File file,String key){
        if(!file.isFile())return null;
        try(SQLiteDatabase db=SQLiteDatabase.openDatabase(file.getPath(),null,SQLiteDatabase.OPEN_READONLY);Cursor c=db.rawQuery("SELECT document FROM records WHERE key=?",new String[]{key})){return c.moveToFirst()?c.getString(0):null;}
    }
    static void put(Context context,String key,String document){put(file(context),key,document);}
    static void put(File file,String key,String document){
        if(!file.getParentFile().isDirectory()&&!file.getParentFile().mkdirs())throw new IllegalStateException("Cannot create catalog folder");
        try(SQLiteDatabase db=SQLiteDatabase.openOrCreateDatabase(file,null)){
            db.execSQL("PRAGMA synchronous=FULL");if(db.getVersion()==0)db.setVersion(1);db.execSQL("CREATE TABLE IF NOT EXISTS records(key TEXT PRIMARY KEY,document TEXT NOT NULL)");
            db.beginTransaction();try{ContentValues values=new ContentValues();values.put("key",key);values.put("document",document);if(db.insertWithOnConflict("records",null,values,SQLiteDatabase.CONFLICT_REPLACE)<0)throw new IllegalStateException("Catalog write failed");db.setTransactionSuccessful();}finally{db.endTransaction();}
        }
    }
}
