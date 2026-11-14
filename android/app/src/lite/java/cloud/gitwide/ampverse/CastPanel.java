package cloud.gitwide.ampverse;
final class CastPanel extends android.widget.LinearLayout {
 interface Listener {void listen(String url,String name);void stopListening();default void playlist(java.util.List<androidx.media3.common.MediaItem> items){}}
 CastPanel(android.app.Activity a,Appearance c,Listener l){super(a);}void close(){}
}
