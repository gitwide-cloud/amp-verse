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
    @Override public boolean isChildDocument(String parent,String child){return parent.equals("root")&&(child.equals("nested")||child.equals("song-a")||child.equals("song-b"));}
    @Override public boolean onCreate(){return true;}
    @Override public Cursor queryRoots(String[] projection){return new MatrixCursor(new String[]{"root_id"});}
    @Override public Cursor queryDocument(String id,String[] projection){return new MatrixCursor(new String[]{Document.COLUMN_DOCUMENT_ID,Document.COLUMN_MIME_TYPE,Document.COLUMN_DISPLAY_NAME});}
    @Override public Cursor queryChildDocuments(String parent,String[] projection,String sort){
        MatrixCursor rows=new MatrixCursor(new String[]{Document.COLUMN_DOCUMENT_ID,Document.COLUMN_MIME_TYPE,Document.COLUMN_DISPLAY_NAME});
        if(parent.equals("root")){
            rows.addRow(new Object[]{"song-a","audio/mpeg","A.mp3"});
            rows.addRow(new Object[]{"nested",Document.MIME_TYPE_DIR,"Album"});
            rows.addRow(new Object[]{"notes","text/plain","Notes.txt"});
        }else if(parent.equals("nested")){
            rows.addRow(new Object[]{"song-b","application/octet-stream","B.FLAC"});
            rows.addRow(new Object[]{"root",Document.MIME_TYPE_DIR,"Cycle"});
        }
        return rows;
    }
    @Override public ParcelFileDescriptor openDocument(String id,String mode,CancellationSignal cancel)throws FileNotFoundException{throw new FileNotFoundException("Traversal fixture only");}
}
