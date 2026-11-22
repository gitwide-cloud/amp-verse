package cloud.gitwide.ampverse;
import android.content.Context;
import android.net.Uri;
import java.util.concurrent.atomic.AtomicBoolean;

/** Local-only decoder contract. Full's implementation is optional; Lite stays lightweight. */
interface VinylDecoder {
    VinylPcm decode(Context context,Uri uri,long positionMs,AtomicBoolean cancelled)throws Exception;
    static boolean available(){try{Class.forName("cloud.gitwide.ampverse.FullVinylDecoder",false,VinylDecoder.class.getClassLoader());return true;}catch(ClassNotFoundException unavailable){return false;}}
    static VinylDecoder full(){try{return (VinylDecoder)Class.forName("cloud.gitwide.ampverse.FullVinylDecoder").getDeclaredConstructor().newInstance();}catch(ReflectiveOperationException unavailable){return null;}}
}
