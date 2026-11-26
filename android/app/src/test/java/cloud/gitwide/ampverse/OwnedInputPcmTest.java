package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import androidx.media3.common.C;
import androidx.media3.common.audio.AudioProcessor;
import java.nio.*;
import org.junit.Test;

public final class OwnedInputPcmTest {
  @Test
  public void monoIsMappedAndGaplessPaddingNeverEscapes() throws Exception {
    OwnedInputPcm processor = new OwnedInputPcm();
    processor.trim(2, 2);
    processor.configure(new AudioProcessor.AudioFormat(44100, 1, C.ENCODING_PCM_16BIT));
    processor.flush();
    ByteBuffer input = ByteBuffer.allocate(12).order(ByteOrder.LITTLE_ENDIAN);
    for (short value = 1; value <= 6; value++) input.putShort(value);
    input.flip();
    processor.queueInput(input);
    ByteBuffer output = processor.getOutput().order(ByteOrder.LITTLE_ENDIAN);
    short[] samples = new short[output.remaining() / 2];
    for (int i = 0; i < samples.length; i++) samples[i] = output.getShort();
    assertArrayEquals(new short[] {3, 3, 4, 4}, samples);
    processor.queueEndOfStream();
    assertFalse(processor.getOutput().hasRemaining());
    assertTrue(processor.isEnded());
  }

  @Test
  public void paddingSpansBuffersWithoutReorderingStereo() throws Exception {
    OwnedInputPcm processor = new OwnedInputPcm();
    processor.trim(0, 1);
    processor.configure(new AudioProcessor.AudioFormat(48000, 2, C.ENCODING_PCM_16BIT));
    processor.flush();
    processor.queueInput(ByteBuffer.wrap(new byte[] {1, 0, 2, 0}));
    assertFalse(processor.getOutput().hasRemaining());
    processor.queueInput(ByteBuffer.wrap(new byte[] {3, 0, 4, 0}));
    ByteBuffer output = processor.getOutput().order(ByteOrder.LITTLE_ENDIAN);
    assertEquals(1, output.getShort());
    assertEquals(2, output.getShort());
    assertFalse(output.hasRemaining());
  }
}
