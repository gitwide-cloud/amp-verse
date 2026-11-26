package cloud.gitwide.ampverse;
class CastPanel extends android.widget.LinearLayout {
 interface Listener {default boolean playlistSyncAvailable(){return false;}default void playlistSync(CastClient selected){}void listen(String url,String name);void stopListening();default void playlist(java.util.List<androidx.media3.common.MediaItem> items){}}
 CastPanel(android.app.Activity a,Appearance c,Listener l){super(a);}void resumeConnection(){}void close(){} void phoneFeedback(String message){}
}
