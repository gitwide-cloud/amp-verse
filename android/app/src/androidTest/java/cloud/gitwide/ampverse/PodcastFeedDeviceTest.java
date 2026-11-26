package cloud.gitwide.ampverse;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.nio.charset.StandardCharsets;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
public final class PodcastFeedDeviceTest {
 @Test public void feedPreviewIncludesAudioBeforeSubscriptionAndRejectsExternalEntities()throws Exception{
  String xml="<rss><channel><title>Test</title><item><title>Audio sample</title><enclosure url='https://example.test/audio.mp3' type='audio/mpeg'/></item><item><title>Video</title><enclosure url='https://example.test/movie.mp4' type='video/mp4'/></item></channel></rss>";
  org.json.JSONArray items=PodcastsPanel.episodes(xml.getBytes(StandardCharsets.UTF_8));assertEquals(1,items.length());assertEquals("Audio sample",items.getJSONObject(0).getString("title"));
  try{PodcastsPanel.episodes("<!DOCTYPE rss SYSTEM 'file:///private'><rss/>".getBytes(StandardCharsets.UTF_8));fail();}catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("declarations"));}
 }
}
