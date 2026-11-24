package cloud.gitwide.ampverse;

import java.io.IOException;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;

/** One service-owned sender; encoding/networking never run on the master render thread. */
final class PhoneMasterBroadcast implements AutoCloseable {
  interface PacketSink {
    void packet(byte[] mp3, int count) throws IOException;
  }

  interface Encoder extends AutoCloseable {
    void encode(MasterPcmQueue source, AtomicBoolean cancelled, PacketSink sink) throws IOException;

    void cancel();

    @Override
    void close();
  }

  interface Encoders {
    Encoder create() throws IOException;
  }

  private final OwnedPcmMixer mixer;
  private final Encoders encoders;
  private final Consumer<String> events;
  private volatile Job current;
  private volatile String status = "Phone broadcast stopped";
  private boolean closed;

  PhoneMasterBroadcast(OwnedPcmMixer mixer, Encoders encoders, Consumer<String> events) {
    this.mixer = mixer;
    this.encoders = encoders;
    this.events = events;
  }

  synchronized void start(PhoneLiveSession session, BooleanSupplier stillSelected)
      throws IOException {
    start(session, stillSelected, stillSelected);
  }

  synchronized void start(
      PhoneLiveSession session, BooleanSupplier stillSelected, BooleanSupplier eligible)
      throws IOException {
    if (closed || current != null)
      throw new IOException("Stop the current phone broadcast before choosing another source");
    if (!stillSelected.getAsBoolean())
      throw new IOException("Playback or host selection changed; choose the source again");
    Job job = new Job(session, stillSelected, eligible);
    current = job;
    update("Preparing this phone’s master broadcast");
    Thread worker = new Thread(() -> run(job), "amp-phone-broadcast");
    worker.setDaemon(true);
    worker.start();
  }

  boolean active() {
    return current != null;
  }

  String status() {
    return status;
  }

  void stop() {
    Job job = current;
    if (job != null) {
      job.cancelled.set(true);
      job.session.cancel();
      Encoder encoder = job.encoder;
      if (encoder != null) encoder.cancel();
    }
  }

  private void update(String text) {
    if (text.equals(status)) return;
    status = text;
    try {
      events.accept(text);
    } catch (RuntimeException ignored) {
    }
  }

  private void run(Job job) {
    try {
      for (int attempt = 0; !job.cancelled.get(); attempt++) {
        MasterPcmQueue queue = new MasterPcmQueue(OwnedPcmMixer.RATE / 4);
        Encoder encoder = null;
        try {
          encoder = encoders.create();
          job.encoder = encoder;
          mixer.fanout(queue);
          encoder.encode(
              queue,
              job.cancelled,
              (packet, count) -> {
                if (job.cancelled.get())
                  throw new PhoneLiveSession.Rejected("Phone broadcast cancelled");
                if (!job.eligible.getAsBoolean())
                  throw new PhoneLiveSession.Rejected("Phone switched to receiving a station");
                // Claim the host only after a real encoded packet exists. Startup failure preserves
                // its program.
                if (job.session.state() == PhoneLiveSession.State.IDLE) {
                  if (!job.selected.getAsBoolean())
                    throw new PhoneLiveSession.Rejected(
                        "Playback source changed during preparation");
                  job.session.begin();
                } else if (job.session.state() == PhoneLiveSession.State.RECONNECTING) {
                  if (!job.eligible.getAsBoolean())
                    throw new PhoneLiveSession.Rejected("Source changed during reconnect");
                  job.session.resume();
                }
                job.session.send(packet, count);
                update("This phone’s master is broadcasting");
              });
          if (!job.cancelled.get()) throw new IOException("Phone encoder stopped unexpectedly");
          break;
        } catch (IOException error) {
          if (job.cancelled.get()) break;
          if (error instanceof PhoneLiveSession.Rejected
              || attempt >= 2
              || job.session.state() == PhoneLiveSession.State.IDLE
              || job.session.state() == PhoneLiveSession.State.FAILED) throw error;
          if (job.session.state() == PhoneLiveSession.State.LIVE) job.session.restartEncoder();
          update("Reconnecting phone broadcast from current audio");
        } finally {
          mixer.detachFanout(queue);
          job.encoder = null;
          if (encoder != null) encoder.close();
        }
        try {
          Thread.sleep(250L * (attempt + 1));
        } catch (InterruptedException error) {
          Thread.currentThread().interrupt();
          break;
        }
      }
    } catch (IOException | RuntimeException error) {
      if (!job.cancelled.get())
        update("Phone broadcast stopped. Playback continues locally; reconnect explicitly.");
    } finally {
      job.session.finish();
      synchronized (this) {
        if (current == job) current = null;
      }
      if (job.cancelled.get()) update("Phone broadcast stopped");
    }
  }

  @Override
  public synchronized void close() {
    closed = true;
    stop();
  }

  private static final class Job {
    final PhoneLiveSession session;
    final BooleanSupplier selected, eligible;
    final AtomicBoolean cancelled = new AtomicBoolean();
    volatile Encoder encoder;

    Job(PhoneLiveSession session, BooleanSupplier selected, BooleanSupplier eligible) {
      this.session = session;
      this.selected = selected;
      this.eligible = eligible;
    }
  }
}
