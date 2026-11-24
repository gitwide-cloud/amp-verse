package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import java.util.Arrays;
import org.junit.Test;

public final class OwnedPcmMixerTest {
  private static short[] tone(int frames, int value) {
    short[] pcm = new short[frames * 2];
    Arrays.fill(pcm, (short) value);
    return pcm;
  }

  private static void fill(OwnedPcmMixer.Voice voice, int frames, int value) {
    assertEquals(frames, voice.write(tone(frames, value), 0, frames, voice.generation()));
    voice.play();
  }

  @Test
  public void activeOwnerAndPrivateCueControlEveryMasterConsumer() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice player = mixer.voice(OwnedPcmMixer.Source.PLAYER);
    OwnedPcmMixer.Voice a = mixer.voice(OwnedPcmMixer.Source.DECK_A);
    OwnedPcmMixer.Voice pad = mixer.voice(OwnedPcmMixer.Source.SAMPLER);
    fill(player, 4, 1000);
    fill(a, 4, 2000);
    fill(pad, 4, 3000);
    short[] out = new short[4];
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 1000), out);
    assertEquals(4, a.queuedFrames());
    mixer.owner(OwnedPcmMixer.Owner.DJ);
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 5000), out);
    assertEquals(2, player.queuedFrames());
    try {
      mixer.voice(OwnedPcmMixer.Source.PRIVATE_CUE);
      fail("Cue must never enter master");
    } catch (IllegalArgumentException expected) {
    }
  }

  @Test
  public void crossfadeGainAndScratchAreMixedBeforeBothOutputs() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    mixer.owner(OwnedPcmMixer.Owner.DJ);
    OwnedPcmMixer.Voice a = mixer.voice(OwnedPcmMixer.Source.DECK_A);
    OwnedPcmMixer.Voice b = mixer.voice(OwnedPcmMixer.Source.DECK_B);
    OwnedPcmMixer.Voice scratch = mixer.voice(OwnedPcmMixer.Source.SCRATCH_A);
    fill(a, 8, 1000);
    fill(b, 8, 3000);
    a.gain((float) Math.cos(Math.PI / 4));
    b.gain((float) Math.sin(Math.PI / 4));
    short[] out = new short[4];
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 2828), out);
    fill(scratch, 2, 4000);
    scratch.gain(.25f);
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 3121), out);
    assertEquals("Normal deck must not run underneath scratch", 6, a.queuedFrames());
    // A held but starved platter stays silent, rather than leaking its deck.
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 2121), out);
    scratch.close();
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 2828), out);
  }

  @Test
  public void exactPostMixFramesAreCopiedIntoTheBoundedEncoderQueue() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice voice = mixer.voice(OwnedPcmMixer.Source.PLAYER);
    short[] source = {10, -10, 20, -20, 30, -30, 40, -40};
    assertEquals(4, voice.write(source, 0, 4, voice.generation()));
    voice.play();
    MasterPcmQueue queue = new MasterPcmQueue(4);
    mixer.fanout(queue);
    short[] local = new short[8];
    mixer.render(local, 4);
    assertArrayEquals(source, local);
    Arrays.fill(local, (short) 0);
    short[] encodedInput = new short[8];
    assertEquals(4, queue.read(encodedInput, 4));
    assertArrayEquals(source, encodedInput);
  }

  @Test
  public void CongestedOrFailedExportNeverInterruptsLocalOutput() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice voice = mixer.voice(OwnedPcmMixer.Source.PLAYER);
    fill(voice, 8, 700);
    MasterPcmQueue queue = new MasterPcmQueue(1);
    mixer.fanout(queue);
    short[] out = new short[4];
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 700), out);
    assertEquals(2, mixer.rejectedFanoutFrames());
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 700), out);
    assertEquals(0, queue.frames());
    queue.close();
    mixer.fanout(queue);
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 700), out);
    assertEquals(4, mixer.rejectedFanoutFrames());
  }

  @Test
  public void queueBackpressureAndSeekGenerationCannotReuseOldDecoderWork() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice voice = mixer.voice(OwnedPcmMixer.Source.PLAYER, 3);
    long original = voice.generation();
    assertEquals(3, voice.write(tone(5, 100), 0, 5, original));
    assertEquals(0, voice.writableFrames());
    voice.play();
    mixer.render(new short[4], 2);
    assertEquals(2, voice.writableFrames());
    long next = voice.flush();
    assertNotEquals(original, next);
    assertEquals(0, voice.write(tone(2, 100), 0, 2, original));
    assertEquals(2, voice.write(tone(2, 200), 0, 2, next));
    short[] out = new short[4];
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 200), out);
  }

  @Test
  public void pauseSeekAndEndUsePresentedFramesInsteadOfWallTime() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice voice = mixer.voice(OwnedPcmMixer.Source.PLAYER);
    fill(voice, 5, 100);
    mixer.render(new short[2], 1);
    assertEquals(0, voice.positionFrames(0));
    assertEquals(1, voice.positionFrames(1));
    voice.pause();
    short[] silent = new short[4];
    mixer.render(silent, 2);
    assertArrayEquals(new short[4], silent);
    assertEquals(1, voice.positionFrames(3));
    assertEquals(4, voice.queuedFrames());
    voice.play();
    voice.end();
    mixer.render(new short[8], 4);
    assertFalse(voice.drained(6));
    assertTrue(voice.drained(7));
    assertEquals(5, voice.positionFrames(7));
    voice.flush();
    assertEquals(0, voice.positionFrames(7));
    assertFalse(voice.drained(7));
  }

  @Test
  public void underflowDoesNotAdvanceMediaPositionAcrossGeneratedSilence() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice voice = mixer.voice(OwnedPcmMixer.Source.PLAYER);
    fill(voice, 2, 100);
    voice.end();
    short[] out = new short[8];
    mixer.render(out, 4);
    assertArrayEquals(new short[] {100, 100, 100, 100, 0, 0, 0, 0}, out);
    assertEquals(2, voice.positionFrames(4));
    assertTrue(voice.drained(2));
  }

  @Test
  public void voiceAndQueueBoundsRemainFixedAndClosedVoicesCannotReturn() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice first = null;
    for (int i = 0; i < OwnedPcmMixer.MAX_VOICES; i++) {
      OwnedPcmMixer.Voice v = mixer.voice(OwnedPcmMixer.Source.SAMPLER, 1);
      if (i == 0) first = v;
    }
    try {
      mixer.voice(OwnedPcmMixer.Source.SAMPLER, 1);
      fail("Voice bound");
    } catch (IllegalStateException expected) {
    }
    long old = first.generation();
    first.close();
    assertEquals(0, first.write(tone(1, 100), 0, 1, old));
    assertNotNull(mixer.voice(OwnedPcmMixer.Source.SAMPLER, 1));
    try {
      mixer.voice(OwnedPcmMixer.Source.PLAYER, OwnedPcmMixer.RATE + 1);
      fail("Queue bound");
    } catch (IllegalArgumentException expected) {
    }
    MasterPcmQueue q = new MasterPcmQueue(2);
    assertFalse(q.offer(new short[2], Integer.MAX_VALUE, 0));
  }

  @Test
  public void exportRejectsDiscontinuousFramesRatherThanSilentlyJoiningThem() {
    MasterPcmQueue q = new MasterPcmQueue(8);
    assertTrue(q.offer(tone(2, 100), 2, 20));
    assertFalse(q.offer(tone(2, 200), 2, 24));
    assertTrue(q.offer(tone(2, 200), 2, 22));
    short[] out = new short[8];
    assertEquals(4, q.read(out, 4));
    assertArrayEquals(new short[] {100, 100, 100, 100, 200, 200, 200, 200}, out);
    q.close();
    assertFalse(q.offer(tone(1, 100), 1, 24));
    assertEquals(0, q.frames());
  }

  @Test
  public void stalePresentationClockIsExplicitInsteadOfSeekingToZero() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice voice = mixer.voice(OwnedPcmMixer.Source.PLAYER);
    fill(voice, 140, 100);
    short[] out = new short[2];
    for (int i = 0; i < 140; i++) mixer.render(out, 1);
    assertEquals(-1, voice.positionFrames(1));
    assertEquals(140, voice.positionFrames(140));
  }

  @Test
  public void stalledConsumerMonitorCannotBlockRender() throws Exception {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice voice = mixer.voice(OwnedPcmMixer.Source.PLAYER);
    fill(voice, 8, 400);
    MasterPcmQueue queue = new MasterPcmQueue(8);
    mixer.fanout(queue);
    java.util.concurrent.CountDownLatch locked = new java.util.concurrent.CountDownLatch(1),
        release = new java.util.concurrent.CountDownLatch(1);
    Thread stalled =
        new Thread(
            () -> {
              synchronized (queue) {
                synchronized (mixer) {
                  synchronized (voice) {
                    locked.countDown();
                    try {
                      release.await();
                    } catch (InterruptedException e) {
                      Thread.currentThread().interrupt();
                    }
                  }
                }
              }
            });
    stalled.start();
    assertTrue(locked.await(2, java.util.concurrent.TimeUnit.SECONDS));
    java.util.concurrent.FutureTask<short[]> render =
        new java.util.concurrent.FutureTask<>(
            () -> {
              short[] out = new short[8];
              mixer.render(out, 4);
              return out;
            });
    Thread thread = new Thread(render);
    thread.start();
    try {
      assertArrayEquals(tone(4, 400), render.get(1, java.util.concurrent.TimeUnit.SECONDS));
    } finally {
      release.countDown();
      stalled.join(2000);
      thread.join(2000);
    }
  }

  @Test
  public void headroomPolicyMakesSaturationObservable() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    mixer.owner(OwnedPcmMixer.Owner.DJ);
    fill(mixer.voice(OwnedPcmMixer.Source.DECK_A), 4, 25000);
    fill(mixer.voice(OwnedPcmMixer.Source.DECK_B), 4, 25000);
    short[] out = new short[4];
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 32767), out);
    assertEquals(4, mixer.clippedSamples());
    mixer.masterGain(.5f);
    MasterPcmQueue queue = new MasterPcmQueue(4);
    mixer.fanout(queue);
    mixer.render(out, 2);
    assertArrayEquals(tone(2, 25000), out);
    assertEquals(4, mixer.clippedSamples());
    short[] network = new short[4];
    assertEquals(2, queue.read(network, 2));
    assertArrayEquals(out, network);
  }

  @Test
  public void spscRingPreservesSamplesUnderConcurrentPressure() throws Exception {
    MasterPcmQueue queue = new MasterPcmQueue(31);
    java.util.concurrent.atomic.AtomicReference<Throwable> error =
        new java.util.concurrent.atomic.AtomicReference<>();
    Thread producer =
        new Thread(
            () -> {
              try {
                for (int i = 0; i < 10000; i++) {
                  short[] sample = {(short) i, (short) -i};
                  while (!queue.offer(sample, 1, i)) Thread.yield();
                }
              } catch (Throwable t) {
                error.set(t);
              }
            });
    producer.start();
    short[] sample = new short[2];
    long deadline = System.nanoTime() + java.util.concurrent.TimeUnit.SECONDS.toNanos(3);
    for (int i = 0; i < 10000; i++) {
      while (queue.read(sample, 1) == 0) {
        if (System.nanoTime() > deadline) fail("Ring stopped progressing");
        Thread.yield();
      }
      assertEquals((short) i, sample[0]);
      assertEquals((short) -i, sample[1]);
    }
    producer.join(2000);
    assertFalse(producer.isAlive());
    assertNull(error.get());
    assertEquals(0, queue.frames());
  }

  @Test
  public void onlyActiveOwnedDecodedPcmCanQualifySenderStartup() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedPcmMixer.Voice player = mixer.voice(OwnedPcmMixer.Source.PLAYER);
    assertFalse(mixer.hasOwnedPcm());
    player.write(tone(2, 0), 0, 2, player.generation());
    assertFalse(mixer.hasOwnedPcm());
    player.play();
    assertTrue(mixer.hasOwnedPcm());
    player.pause();
    assertFalse(mixer.hasOwnedPcm());
    mixer.owner(OwnedPcmMixer.Owner.DJ);
    player.play();
    assertFalse(mixer.hasOwnedPcm());
    OwnedPcmMixer.Voice deck = mixer.voice(OwnedPcmMixer.Source.DECK_A);
    fill(deck, 2, 100);
    assertTrue(mixer.hasOwnedPcm());
    deck.flush();
    assertFalse(mixer.hasOwnedPcm());
  }

  public static void main(String[] args) {
    org.junit.runner.JUnitCore.main(OwnedPcmMixerTest.class.getName());
  }
}
