package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import org.junit.Test;

public final class PhoneMp3WarmupTest {
  static byte[] frame() {
    byte[] frame = new byte[576];
    frame[0] = (byte) 255;
    frame[1] = (byte) 251;
    frame[2] = (byte) 180;
    return frame;
  }

  @Test
  public void partialFrameCannotClaimHost() {
    PhoneMp3Warmup prefix = new PhoneMp3Warmup();
    try {
      prefix.append(new byte[] {(byte) 255, (byte) 251}, 2);
      assertFalse(prefix.ready());
      byte[] rest = java.util.Arrays.copyOfRange(frame(), 2, 576);
      prefix.append(rest, rest.length);
      assertTrue(prefix.ready());
      assertArrayEquals(frame(), prefix.take());
    } catch (Exception error) {
      throw new AssertionError(error);
    }
  }

  @Test
  public void id3HeaderAndFakeSyncInMetadataAreNotAudio() throws Exception {
    PhoneMp3Warmup prefix = new PhoneMp3Warmup();
    byte[] tag = new byte[610];
    tag[0] = 'I';
    tag[1] = 'D';
    tag[2] = '3';
    tag[3] = 4;
    tag[8] = 4;
    tag[9] = 88;
    System.arraycopy(frame(), 0, tag, 10, 576);
    prefix.append(tag, tag.length);
    assertFalse(prefix.ready());
    prefix.append(frame(), 576);
    assertTrue(prefix.ready());
    assertEquals(1186, prefix.take().length);
  }

  @Test
  public void invalidAndOversizedWarmupFailWithinFixedBudget() throws Exception {
    PhoneMp3Warmup prefix = new PhoneMp3Warmup();
    prefix.append(new byte[8192], 8192);
    assertFalse(prefix.ready());
    prefix.append(new byte[8192], 8192);
    try {
      prefix.append(new byte[1], 1);
      fail();
    } catch (java.io.IOException expected) {
    }
    try {
      prefix.take();
      fail();
    } catch (java.io.IOException expected) {
    }
  }
}
