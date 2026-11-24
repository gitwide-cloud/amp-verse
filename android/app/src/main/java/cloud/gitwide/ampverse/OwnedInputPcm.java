package cloud.gitwide.ampverse;

import androidx.media3.common.C;
import androidx.media3.common.audio.BaseAudioProcessor;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/** Bounded gapless trimming and mono/stereo mapping before EQ and Sonic. */
@androidx.annotation.OptIn(markerClass = androidx.media3.common.util.UnstableApi.class)
final class OwnedInputPcm extends BaseAudioProcessor {
  private int delay, padding, channels, skip, tailFrames, tailRead;
  private boolean checkpoint, ending;

  void checkpoint() {
    checkpoint = true;
  }

  private short[] tail = new short[0];

  void trim(int delay, int padding) {
    if (delay < 0 || padding < 0 || delay > 48000 || padding > 48000)
      throw new IllegalArgumentException("Unsupported encoder trim");
    this.delay = delay;
    this.padding = padding;
  }

  @Override
  protected AudioFormat onConfigure(AudioFormat input) throws UnhandledAudioFormatException {
    if (input.encoding != C.ENCODING_PCM_16BIT
        || (input.channelCount != 1 && input.channelCount != 2))
      throw new UnhandledAudioFormatException(input);
    channels = input.channelCount;
    return new AudioFormat(input.sampleRate, 2, C.ENCODING_PCM_16BIT);
  }

  @Override
  protected void onFlush() {
    skip = delay;
    tailFrames = 0;
    tailRead = 0;
    checkpoint = false;
    ending = false;
    tail = new short[padding * 2];
  }

  @Override
  protected void onQueueEndOfStream() {
    ending = true;
  }

  @Override
  public ByteBuffer getOutput() {
    ByteBuffer pending = super.getOutput();
    if (pending.hasRemaining()) return pending;
    if (ending && checkpoint && tailFrames > 0) {
      ByteBuffer output = replaceOutputBuffer(tailFrames * 4).order(ByteOrder.LITTLE_ENDIAN);
      int oldest = (tailRead - tailFrames + padding) % padding;
      for (int i = 0; i < tailFrames; i++) {
        int at = (oldest + i) % padding;
        output.putShort(tail[at * 2]).putShort(tail[at * 2 + 1]);
      }
      tailFrames = 0;
      output.flip();
      return super.getOutput();
    }
    return pending;
  }

  @Override
  public boolean isEnded() {
    return super.isEnded() && !(ending && checkpoint && tailFrames > 0);
  }

  @Override
  public void queueInput(ByteBuffer input) {
    input.order(ByteOrder.LITTLE_ENDIAN);
    ByteBuffer output =
        replaceOutputBuffer(input.remaining() * 2 / channels).order(ByteOrder.LITTLE_ENDIAN);
    while (input.remaining() >= channels * 2) {
      short left = input.getShort(), right = channels == 2 ? input.getShort() : left;
      if (skip > 0) {
        skip--;
        continue;
      }
      if (padding == 0) {
        output.putShort(left).putShort(right);
        continue;
      }
      if (tailFrames == padding) {
        output.putShort(tail[tailRead * 2]).putShort(tail[tailRead * 2 + 1]);
      } else tailFrames++;
      tail[tailRead * 2] = left;
      tail[tailRead * 2 + 1] = right;
      tailRead = (tailRead + 1) % padding;
    }
    output.flip();
  }
}
