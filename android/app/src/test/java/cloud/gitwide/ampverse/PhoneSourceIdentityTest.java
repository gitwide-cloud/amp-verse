package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import org.junit.Test;

public final class PhoneSourceIdentityTest {
  @Test
  public void internetRadioAndLocalFilesRemainLegitimateOwnedSources() {
    assertFalse(
        PhoneSourceIdentity.receiver("https://radio.example/listen.mp3", "http://amp.local:8787"));
    assertFalse(PhoneSourceIdentity.receiver("file:///music/song.mp3", "http://amp.local:8787"));
    assertFalse(PhoneSourceIdentity.receiver("https://radio.example/live.m3u8", ""));
  }

  @Test
  public void ampReceiversCannotLoopBackEvenAcrossAlternatePaths() {
    assertTrue(
        PhoneSourceIdentity.receiver(
            "http://amp.local:8787/amp-protected.mp3?server=s&station=t", ""));
    assertTrue(
        PhoneSourceIdentity.receiver(
            "http://amp.local:8787/api/audio/hls/master.m3u8?server=s&station=t", ""));
    assertTrue(PhoneSourceIdentity.receiver("http://alternate:8787/listen.mp3?amp_receiver=1", ""));
    assertTrue(PhoneSourceIdentity.receiver("http://AMP.local:80/listen.mp3", "http://amp.local"));
    assertFalse(
        PhoneSourceIdentity.receiver("http://amp.local:8788/listen.mp3", "http://amp.local:8787"));
  }
}
