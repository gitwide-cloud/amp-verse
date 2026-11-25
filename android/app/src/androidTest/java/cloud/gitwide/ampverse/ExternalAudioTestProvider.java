package cloud.gitwide.ampverse;
import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.*;
import java.nio.*;
/** Isolated, temporary-grant provider; no user media or storage permissions. */
public final class ExternalAudioTestProvider extends ContentProvider {
    @Override public boolean onCreate(){return true;}
    @Override public String getType(Uri uri){return "audio/wav";}
    @Override public Cursor query(Uri uri,String[] projection,String selection,String[] args,String sort){MatrixCursor rows=new MatrixCursor(new String[]{OpenableColumns.DISPLAY_NAME});rows.addRow(new Object[]{"Open-with fixture.wav"});return rows;}
    @Override public ParcelFileDescriptor openFile(Uri uri,String mode)throws FileNotFoundException{
        if(!"r".equals(mode)||!"/song".equals(uri.getPath()))throw new FileNotFoundException();
        try{File file=new File(getContext().getCacheDir(),"open-with-fixture.wav");
            if(!file.exists()){int frames=8000*30;ByteBuffer bytes=ByteBuffer.allocate(44+frames*2).order(ByteOrder.LITTLE_ENDIAN);
                bytes.put("RIFF".getBytes()).putInt(36+frames*2).put("WAVEfmt ".getBytes()).putInt(16).putShort((short)1).putShort((short)1).putInt(8000).putInt(16000).putShort((short)2).putShort((short)16).put("data".getBytes()).putInt(frames*2);
                for(int i=0;i<frames;i++)bytes.putShort((short)(Math.sin(i*2*Math.PI*440/8000)*2000));java.nio.file.Files.write(file.toPath(),bytes.array());}
            return ParcelFileDescriptor.open(file,ParcelFileDescriptor.MODE_READ_ONLY);
        }catch(IOException error){throw new FileNotFoundException(error.toString());}
    }
    @Override public Uri insert(Uri uri,ContentValues values){throw new UnsupportedOperationException();}
    @Override public int update(Uri uri,ContentValues values,String where,String[] args){throw new UnsupportedOperationException();}
    @Override public int delete(Uri uri,String where,String[] args){throw new UnsupportedOperationException();}
}
