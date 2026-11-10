package cloud.gitwide.ampverse;

import android.database.Cursor;
import android.database.MatrixCursor;
import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;
import android.provider.DocumentsProvider;
import android.provider.DocumentsContract.Document;
import java.io.FileNotFoundException;

/** Test-only provider: no real phone files, permissions or library entries. */
public class FolderTestProvider extends DocumentsProvider {
    @Override public boolean isChildDocument(String parent,String child){return parent.equals("root")&&(child.equals("nested")||child.equals("song-a")||child.equals("song-b")||child.startsWith("meta"));}
    @Override public boolean onCreate(){return true;}
    @Override public Cursor queryRoots(String[] projection){return new MatrixCursor(new String[]{"root_id"});}
    @Override public Cursor queryDocument(String id,String[] projection){return new MatrixCursor(new String[]{Document.COLUMN_DOCUMENT_ID,Document.COLUMN_MIME_TYPE,Document.COLUMN_DISPLAY_NAME});}
    @Override public Cursor queryChildDocuments(String parent,String[] projection,String sort){
        MatrixCursor rows=new MatrixCursor(new String[]{Document.COLUMN_DOCUMENT_ID,Document.COLUMN_MIME_TYPE,Document.COLUMN_DISPLAY_NAME});
        if(parent.equals("root")){
            rows.addRow(new Object[]{"song-a","audio/mpeg","A.mp3"});
            rows.addRow(new Object[]{"nested",Document.MIME_TYPE_DIR,"Album"});
            rows.addRow(new Object[]{"notes","text/plain","Notes.txt"});
            rows.addRow(new Object[]{"meta",Document.MIME_TYPE_DIR,"ampdj"});
        }else if(parent.equals("nested")){
            rows.addRow(new Object[]{"song-b","application/octet-stream","B.FLAC"});
            rows.addRow(new Object[]{"root",Document.MIME_TYPE_DIR,"Cycle"});
        }
        if(parent.equals("meta")){rows.addRow(new Object[]{"meta-tracks",Document.MIME_TYPE_DIR,"tracks"});rows.addRow(new Object[]{"meta-trap","audio/mpeg","Skip.mp3"});}
        if(parent.equals("meta-tracks")){rows.addRow(new Object[]{"meta-a","application/json","A.mp3.json"});rows.addRow(new Object[]{"meta-album",Document.MIME_TYPE_DIR,"Album"});}
        if(parent.equals("meta-album"))rows.addRow(new Object[]{"meta-b","application/json","B.FLAC.json"});
        return rows;
    }
    @Override public ParcelFileDescriptor openDocument(String id,String mode,CancellationSignal cancel)throws FileNotFoundException{if(!id.equals("meta-a")&&!id.equals("meta-b"))throw new FileNotFoundException("Traversal fixture only");
        try{java.io.File file=new java.io.File(getContext().getCacheDir(),"preparation-fixture.json");java.nio.file.Files.write(file.toPath(),"{\"cue_in\":7.25,\"bpm\":128,\"manual_override\":true}".getBytes(java.nio.charset.StandardCharsets.UTF_8));return ParcelFileDescriptor.open(file,ParcelFileDescriptor.MODE_READ_ONLY);}catch(java.io.IOException error){throw new FileNotFoundException(error.toString());}}
}
