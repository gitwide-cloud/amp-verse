package cloud.gitwide.ampverse.lab;
import org.junit.Test;
import static org.junit.Assert.*;
public class YouTubeUrlTest {
 @Test public void playlistParametersNeverSelectAPlaylist(){assertEquals("https://www.youtube.com/watch?v=BaW_jenozKc",YouTubeUrl.normalize("https://www.youtube.com/watch?v=BaW_jenozKc&list=anything&index=5"));}
 @Test public void shortMobileAndLiveLinksNormalize(){for(String url:new String[]{"https://youtu.be/BaW_jenozKc?t=2","https://m.youtube.com/watch?v=BaW_jenozKc","https://youtube.com/shorts/BaW_jenozKc","https://youtube.com/live/BaW_jenozKc"})assertEquals("https://www.youtube.com/watch?v=BaW_jenozKc",YouTubeUrl.normalize(url));}
 @Test public void rejectsPlaylistOnlyAndUntrustedSources(){for(String url:new String[]{"https://youtube.com/playlist?list=abc","https://youtube.com.evil.test/watch?v=BaW_jenozKc","https://youtube.com@evil.test/watch?v=BaW_jenozKc","file:///tmp/audio","https://youtube.com/watch?v=BaW_jenozKc&v=AAAAAAAAAAA","https://youtube.com/watch?v=short","http://youtube.com/watch?v=BaW_jenozKc","https://youtube.com:444/watch?v=BaW_jenozKc"}){try{YouTubeUrl.normalize(url);fail(url);}catch(IllegalArgumentException expected){assertFalse(expected.getMessage().isEmpty());}}}
}
