package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import java.io.IOException;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import org.junit.Test;

public final class PhoneMasterBroadcastTest {
  static final PhoneLiveSession.Identity ID =
      new PhoneLiveSession.Identity("server", "station", "library");

  static class Transport implements PhoneLiveSession.Transport {
    volatile int starts, uploads, stops;

    public PhoneLiveSession.Identity verify() {
      return ID;
    }

    public PhoneLiveSession.Started start(String previous) {
      starts++;
      return new PhoneLiveSession.Started("owned-" + starts, 0, true, true);
    }

    public long upload(String token, long sequence, byte[] data) {
      uploads++;
      return sequence + 1;
    }

    public void stop(String token) {
      stops++;
    }

    public void abort() {}
  }

  static void await(java.util.function.BooleanSupplier condition) throws Exception {
    long end = System.nanoTime() + TimeUnit.SECONDS.toNanos(3);
    while (!condition.getAsBoolean()) {
      if (System.nanoTime() > end) fail("Broadcast worker stalled");
      Thread.sleep(2);
    }
  }

  @Test
  public void encoderStartupFailureNeverClaimsOrStopsExistingHostProgram() throws Exception {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    Transport transport = new Transport();
    PhoneMasterBroadcast broadcast =
        new PhoneMasterBroadcast(
            mixer,
            () -> {
              throw new IOException("Unsupported codec");
            },
            s -> {});
    broadcast.start(new PhoneLiveSession(ID, transport), () -> true);
    await(() -> !broadcast.active());
    assertEquals(0, transport.starts);
    assertEquals(0, transport.stops);
    broadcast.close();
  }

  @Test
  public void sourceChangedDuringPreparationNeverClaimsHost() throws Exception {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    Transport transport = new Transport();
    AtomicBoolean selected = new AtomicBoolean(true);
    PhoneMasterBroadcast broadcast =
        new PhoneMasterBroadcast(
            mixer,
            () ->
                new PhoneMasterBroadcast.Encoder() {
                  public void encode(
                      MasterPcmQueue pcm,
                      AtomicBoolean stop,
                      PhoneMasterBroadcast.PacketSink packets)
                      throws IOException {
                    selected.set(false);
                    packets.packet(new byte[] {1}, 1);
                  }

                  public void cancel() {}

                  public void close() {}
                },
            s -> {});
    broadcast.start(new PhoneLiveSession(ID, transport), selected::get);
    await(() -> !broadcast.active());
    assertEquals(0, transport.starts);
    broadcast.close();
  }

  @Test
  public void oneSenderAndCancellationLeaveLocalRenderAvailable() throws Exception {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    Transport transport = new Transport();
    CountDownLatch encoding = new CountDownLatch(1);
    PhoneMasterBroadcast broadcast =
        new PhoneMasterBroadcast(
            mixer,
            () ->
                new PhoneMasterBroadcast.Encoder() {
                  public void encode(
                      MasterPcmQueue pcm,
                      AtomicBoolean stop,
                      PhoneMasterBroadcast.PacketSink packets)
                      throws IOException {
                    encoding.countDown();
                    short[] data = new short[960];
                    while (!stop.get()) {
                      if (pcm.read(data, 480) > 0) packets.packet(new byte[] {1, 2}, 2);
                      try {
                        Thread.sleep(2);
                      } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                        return;
                      }
                    }
                  }

                  public void cancel() {}

                  public void close() {}
                },
            s -> {});
    broadcast.start(new PhoneLiveSession(ID, transport), () -> true);
    assertTrue(encoding.await(2, TimeUnit.SECONDS));
    try {
      broadcast.start(new PhoneLiveSession(ID, transport), () -> true);
      fail();
    } catch (IOException expected) {
    }
    OwnedPcmMixer.Voice voice = mixer.voice(OwnedPcmMixer.Source.PLAYER);
    short[] data = new short[960];
    java.util.Arrays.fill(data, (short) 900);
    voice.write(data, 0, 480, voice.generation());
    voice.play();
    short[] local = new short[960];
    mixer.render(local, 480);
    assertArrayEquals(data, local);
    await(() -> transport.uploads > 0);
    broadcast.stop();
    await(() -> !broadcast.active());
    assertEquals(1, transport.starts);
    assertEquals(1, transport.stops);
    broadcast.close();
  }

  @Test
  public void switchingToReceiveStopsOwnedSenderBeforeSecondPacket() throws Exception {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    Transport transport = new Transport();
    AtomicBoolean eligible = new AtomicBoolean(true);
    PhoneMasterBroadcast broadcast =
        new PhoneMasterBroadcast(
            mixer,
            () ->
                new PhoneMasterBroadcast.Encoder() {
                  public void encode(
                      MasterPcmQueue pcm,
                      AtomicBoolean stop,
                      PhoneMasterBroadcast.PacketSink packets)
                      throws IOException {
                    packets.packet(new byte[] {1}, 1);
                    eligible.set(false);
                    packets.packet(new byte[] {2}, 1);
                  }

                  public void cancel() {}

                  public void close() {}
                },
            s -> {});
    broadcast.start(new PhoneLiveSession(ID, transport), () -> true, eligible::get);
    await(() -> !broadcast.active());
    assertEquals(1, transport.starts);
    assertEquals(1, transport.uploads);
    assertEquals(1, transport.stops);
    broadcast.close();
  }

  @Test
  public void encoderRecoveryUsesFreshPacketsAndGuardedOwnerResume() throws Exception {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    AtomicInteger encoders = new AtomicInteger();
    AtomicReference<String> previous = new AtomicReference<>();
    Transport transport =
        new Transport() {
          @Override
          public PhoneLiveSession.Started start(String token) {
            if (token != null) previous.set(token);
            return super.start(token);
          }
        };
    PhoneMasterBroadcast broadcast =
        new PhoneMasterBroadcast(
            mixer,
            () -> {
              int attempt = encoders.incrementAndGet();
              return new PhoneMasterBroadcast.Encoder() {
                public void encode(
                    MasterPcmQueue pcm, AtomicBoolean stop, PhoneMasterBroadcast.PacketSink packets)
                    throws IOException {
                  packets.packet(new byte[] {(byte) attempt}, 1);
                  if (attempt == 1) throw new IOException("Fresh stream required");
                  while (!stop.get())
                    try {
                      Thread.sleep(2);
                    } catch (InterruptedException e) {
                      Thread.currentThread().interrupt();
                      return;
                    }
                }

                public void cancel() {}

                public void close() {}
              };
            },
            s -> {});
    broadcast.start(new PhoneLiveSession(ID, transport), () -> true);
    await(() -> transport.uploads == 2);
    broadcast.stop();
    await(() -> !broadcast.active());
    assertEquals(2, encoders.get());
    assertEquals(2, transport.starts);
    assertEquals("owned-1", previous.get());
    assertEquals(1, transport.stops);
    broadcast.close();
  }

  @Test
  public void repeatedStopDoesNotAbortItsOwnGuardedCleanup() throws Exception {
    AtomicInteger aborts = new AtomicInteger();
    CountDownLatch cleaning = new CountDownLatch(1), release = new CountDownLatch(1);
    Transport transport =
        new Transport() {
          public void abort() {
            aborts.incrementAndGet();
          }

          public void stop(String token) {
            cleaning.countDown();
            try {
              release.await(2, TimeUnit.SECONDS);
            } catch (InterruptedException error) {
              Thread.currentThread().interrupt();
            }
            super.stop(token);
          }
        };
    PhoneMasterBroadcast broadcast =
        new PhoneMasterBroadcast(
            new OwnedPcmMixer(),
            () ->
                new PhoneMasterBroadcast.Encoder() {
                  public void encode(
                      MasterPcmQueue pcm,
                      AtomicBoolean stop,
                      PhoneMasterBroadcast.PacketSink packets)
                      throws IOException {
                    packets.packet(new byte[] {1}, 1);
                    while (!stop.get())
                      try {
                        Thread.sleep(2);
                      } catch (InterruptedException error) {
                        Thread.currentThread().interrupt();
                        return;
                      }
                  }

                  public void cancel() {}

                  public void close() {}
                },
            s -> {});
    try {
      broadcast.start(new PhoneLiveSession(ID, transport), () -> true);
      await(() -> transport.uploads == 1);
      broadcast.stop();
      assertTrue(cleaning.await(2, TimeUnit.SECONDS));
      broadcast.stop();
      broadcast.close();
      assertEquals(1, aborts.get());
    } finally {
      release.countDown();
      broadcast.close();
      await(() -> !broadcast.active());
    }
    assertEquals(1, transport.stops);
  }
}
