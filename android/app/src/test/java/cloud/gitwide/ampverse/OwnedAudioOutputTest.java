package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import java.util.*;
import java.util.concurrent.*;
import org.junit.Test;

public final class OwnedAudioOutputTest {
  static final class Hardware implements OwnedAudioOutput.Device {
    final List<Short> samples = new ArrayList<>();
    volatile boolean playing;
    volatile int quota = 128;
    final CountDownLatch paused = new CountDownLatch(1), closed = new CountDownLatch(1);

    public synchronized int write(short[] input, int offset, int count) {
      if (!playing) return 0;
      int n = Math.min(Math.min(count, 128), quota - samples.size());
      if (n <= 0) return 0;
      for (int i = 0; i < n; i++) samples.add(input[offset + i]);
      return n;
    }

    public synchronized long head() {
      return samples.size() / 2;
    }

    public int bufferFrames() {
      return 960;
    }

    public void play() {
      playing = true;
    }

    public void pause() {
      playing = false;
      paused.countDown();
    }

    public void flush() {}

    public void release() {
      closed.countDown();
    }

    synchronized int size() {
      return samples.size();
    }
  }

  static void await(java.util.function.BooleanSupplier condition) throws Exception {
    long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2);
    while (!condition.getAsBoolean()) {
      if (System.nanoTime() > deadline) fail("Output stalled");
      Thread.sleep(2);
    }
  }

  @Test
  public void focusInterruptionRetainsPartialBlockAndOneHardwareClock() throws Exception {
    Hardware device = new Hardware();
    OwnedAudioOutput output = new OwnedAudioOutput(() -> device, () -> {});
    OwnedPcmMixer.Voice voice = output.mixer.voice(OwnedPcmMixer.Source.PLAYER);
    short[] source = new short[960];
    for (int i = 0; i < source.length; i++) source[i] = (short) (i + 1);
    voice.write(source, 0, 480, voice.generation());
    voice.play();
    try {
      output.prepare();
      await(() -> device.size() == 128);
      output.suppress(true);
      assertTrue(device.paused.await(2, TimeUnit.SECONDS));
      long frozen = output.presentedFrames();
      Thread.sleep(20);
      assertEquals(128, device.size());
      assertEquals(frozen, output.presentedFrames());
      assertEquals(64, voice.positionFrames(frozen));
      device.quota = 960;
      output.suppress(false);
      await(() -> device.size() == 960);
      await(() -> output.presentedFrames() == 480);
      synchronized (device) {
        for (int i = 0; i < source.length; i++)
          assertEquals(source[i], device.samples.get(i).shortValue());
      }
      assertNull(output.failure());
    } finally {
      output.close();
      assertTrue(device.closed.await(2, TimeUnit.SECONDS));
    }
  }

  @Test
  public void failedHardwarePreparationDoesNotConsumeSource() throws Exception {
    OwnedAudioOutput output =
        new OwnedAudioOutput(
            () -> {
              throw new java.io.IOException("Unavailable");
            },
            () -> {});
    OwnedPcmMixer.Voice voice = output.mixer.voice(OwnedPcmMixer.Source.PLAYER);
    voice.write(new short[960], 0, 480, voice.generation());
    voice.play();
    try {
      output.prepare();
      fail();
    } catch (java.io.IOException expected) {
    }
    assertEquals(480, voice.queuedFrames());
    assertEquals(0, output.mixer.frames());
    output.close();
  }

  @Test
  public void stalledHardwareIsReportedInsteadOfBlockingPlaybackForever() throws Exception {
    Hardware device = new Hardware();
    device.quota = 0;
    OwnedAudioOutput output = new OwnedAudioOutput(() -> device, () -> {}, 20000000L);
    try {
      output.prepare();
      await(() -> output.failure() != null);
      assertFalse(output.ready());
      assertTrue(device.closed.await(2, TimeUnit.SECONDS));
    } finally {
      output.close();
    }
  }
}
