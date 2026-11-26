package cloud.gitwide.ampverse;

import java.io.ByteArrayOutputStream;
import java.io.IOException;

/** Bounded first complete Layer III frame check; ID3 headers alone never claim a host program. */
final class PhoneMp3Warmup {
  private static final int[] MPEG1 = {
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320
  };
  private static final int[] MPEG2 = {0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160};
  private static final int[] RATES = {44100, 48000, 32000};
  private final ByteArrayOutputStream bytes = new ByteArrayOutputStream();
  private boolean ready;

  void append(byte[] input, int count) throws IOException {
    if (count < 1
        || count > input.length
        || count > PhoneLiveSession.MAX_PACKET
        || bytes.size() + count > 16384)
      throw new IOException("MP3 encoder warmup exceeded its bounded prefix");
    bytes.write(input, 0, count);
    byte[] prefix = bytes.toByteArray();
    int start = 0;
    if (prefix.length >= 3 && prefix[0] == 'I' && prefix[1] == 'D' && prefix[2] == '3') {
      if (prefix.length < 10) return;
      int size = 0;
      for (int i = 6; i < 10; i++) {
        if ((prefix[i] & 128) != 0) throw new IOException("Invalid native ID3 size");
        size = (size << 7) | (prefix[i] & 127);
      }
      start = 10 + size + ((prefix[3] == 4 && (prefix[5] & 16) != 0) ? 10 : 0);
      if (start > 8192) throw new IOException("Native MP3 metadata exceeds live warmup budget");
    }
    for (int at = start; at + 4 <= prefix.length && at < 8192; at++) {
      int first = prefix[at] & 255, second = prefix[at + 1] & 255, third = prefix[at + 2] & 255;
      int version = (second >> 3) & 3,
          layer = (second >> 1) & 3,
          rate = (third >> 2) & 3,
          bitrate = (third >> 4) & 15;
      if (first != 255
          || (second & 224) != 224
          || version == 1
          || layer != 1
          || rate == 3
          || bitrate == 0
          || bitrate == 15) continue;
      int sampleRate = RATES[rate] / (version == 3 ? 1 : version == 2 ? 2 : 4);
      int kbps = (version == 3 ? MPEG1 : MPEG2)[bitrate];
      int length = (version == 3 ? 144000 : 72000) * kbps / sampleRate + ((third >> 1) & 1);
      if (at + length <= prefix.length && at + length <= 8192) {
        ready = true;
        return;
      }
    }
  }

  boolean ready() {
    return ready;
  }

  byte[] take() throws IOException {
    if (!ready) throw new IOException("No complete MP3 frame is ready");
    return bytes.toByteArray();
  }
}
