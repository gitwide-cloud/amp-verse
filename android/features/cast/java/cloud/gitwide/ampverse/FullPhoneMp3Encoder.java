package cloud.gitwide.ampverse;

import android.content.Context;
import android.system.*;
import com.arthenica.ffmpegkit.*;
import java.io.*;
import java.util.concurrent.atomic.*;

/** Full-only bounded FIFO encoder. FIFO nodes hold no saved audio files. */
final class FullPhoneMp3Encoder implements PhoneMasterBroadcast.Encoder {
  private static final AtomicReference<Object> LEASE = new AtomicReference<>();
  private final Object ticket = new Object();
  private final String inputPipe, outputPipe;
  private final AtomicBoolean stopped = new AtomicBoolean();
  private final AtomicReference<IOException> failure = new AtomicReference<>();
  private volatile FileDescriptor input, output;
  private volatile FFmpegSession session;
  private volatile boolean released;

  FullPhoneMp3Encoder(Context context) throws IOException {
    if (!LEASE.compareAndSet(null, ticket))
      throw new IOException("Previous phone encoder is still stopping");
    String first = null, second = null;
    try {
      first = FFmpegKitConfig.registerNewFFmpegPipe(context);
      second = FFmpegKitConfig.registerNewFFmpegPipe(context);
      if (first == null || second == null) throw new IOException("Phone encoder pipes unavailable");
      inputPipe = first;
      outputPipe = second;
    } catch (Exception error) {
      if (first != null) FFmpegKitConfig.closeFFmpegPipe(first);
      if (second != null) FFmpegKitConfig.closeFFmpegPipe(second);
      LEASE.compareAndSet(ticket, null);
      throw new IOException("Phone encoder unavailable", error);
    }
  }

  @Override
  public void encode(
      MasterPcmQueue source, AtomicBoolean cancelled, PhoneMasterBroadcast.PacketSink packets)
      throws IOException {
    Thread writer = null;
    try {
      if (cancelled.get() || stopped.get()) return;
      output = Os.open(outputPipe, OsConstants.O_RDONLY | OsConstants.O_NONBLOCK, 0);
      bound(output);
      session =
          FFmpegKit.executeWithArgumentsAsync(
              new String[] {
                "-nostdin",
                "-v",
                "error",
                "-y",
                "-threads",
                "1",
                "-f",
                "s16le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-probesize",
                "32",
                "-analyzeduration",
                "0",
                "-i",
                inputPipe,
                "-c:a",
                "libmp3lame",
                "-b:a",
                "192k",
                "-reservoir",
                "0",
                "-flush_packets",
                "1",
                "-f",
                "mp3",
                outputPipe
              },
              finished -> {
                if (released) LEASE.compareAndSet(ticket, null);
              });
      long opening = System.nanoTime() + 2000000000L;
      while (input == null && !stopped.get() && !cancelled.get()) {
        try {
          input = Os.open(inputPipe, OsConstants.O_WRONLY | OsConstants.O_NONBLOCK, 0);
          bound(input);
        } catch (ErrnoException unavailable) {
          if (unavailable.errno != OsConstants.ENXIO) throw unavailable;
          if (System.nanoTime() > opening) throw new IOException("Phone encoder input timed out");
          pause();
        }
      }
      if (input == null) return;
      writer = new Thread(() -> feed(source, cancelled), "amp-phone-encoder-pcm");
      writer.setDaemon(true);
      writer.start();
      byte[] packet = new byte[PhoneLiveSession.MAX_PACKET];
      long last = System.nanoTime();
      while (!stopped.get() && !cancelled.get()) {
        IOException problem = failure.get();
        if (problem != null) throw problem;
        int count;
        try {
          FileDescriptor descriptor = output;
          if (descriptor == null) break;
          count = Os.read(descriptor, packet, 0, packet.length);
        } catch (ErrnoException wait) {
          if (wait.errno != OsConstants.EAGAIN) throw wait;
          count = 0;
        }
        if (count > 0) {
          packets.packet(packet, count);
          last = System.nanoTime();
        } else {
          if (terminal(session) || System.nanoTime() - last > 3000000000L)
            throw new IOException("Phone encoder stopped producing live MP3");
          pause();
        }
      }
      IOException problem = failure.get();
      if (problem != null && !cancelled.get()) throw problem;
    } catch (ErrnoException | InterruptedIOException error) {
      if (!cancelled.get() && !stopped.get())
        throw new IOException("Phone encoder pipe failed", error);
    } finally {
      cancel();
      if (writer != null)
        try {
          writer.join(300);
        } catch (InterruptedException interrupted) {
          Thread.currentThread().interrupt();
        }
    }
  }

  private void feed(MasterPcmQueue source, AtomicBoolean cancelled) {
    short[] samples = new short[960];
    byte[] pcm = new byte[1920];
    try {
      while (!stopped.get() && !cancelled.get()) {
        if (source.rejectedFrames() > 0)
          throw new IOException("Phone master buffer exceeded latency budget");
        int frames = source.read(samples, 480);
        if (frames == 0) {
          pause();
          continue;
        }
        for (int i = 0; i < frames * 2; i++) {
          pcm[i * 2] = (byte) samples[i];
          pcm[i * 2 + 1] = (byte) (samples[i] >> 8);
        }
        int offset = 0;
        long deadline = System.nanoTime() + 250000000L;
        while (offset < frames * 4 && !stopped.get() && !cancelled.get()) {
          try {
            FileDescriptor descriptor = input;
            if (descriptor == null) return;
            offset += Os.write(descriptor, pcm, offset, frames * 4 - offset);
          } catch (ErrnoException wait) {
            if (wait.errno != OsConstants.EAGAIN) throw wait;
            if (System.nanoTime() > deadline)
              throw new IOException("Phone encoder fell behind live audio");
            pause();
          }
        }
      }
    } catch (Exception error) {
      if (!cancelled.get() && !stopped.get()) {
        failure.compareAndSet(null, new IOException("Phone encoder could not keep up", error));
        stopped.set(true);
      }
    }
  }

  private static void bound(FileDescriptor pipe) throws ErrnoException, IOException {
    // Linux F_SETPIPE_SZ: Android kernels round to 4/16 KiB pages. Reject larger buffering.
    int capacity = Os.fcntlInt(pipe, 1031, 4096);
    if (capacity > 16384 || capacity < 4096)
      throw new IOException("Bounded native pipe capacity unavailable");
  }

  private static void pause() throws InterruptedIOException {
    try {
      Thread.sleep(2);
    } catch (InterruptedException error) {
      Thread.currentThread().interrupt();
      throw new InterruptedIOException("Phone encoder cancelled");
    }
  }

  private static boolean terminal(FFmpegSession value) {
    return value != null
        && value.getState() != SessionState.CREATED
        && value.getState() != SessionState.RUNNING;
  }

  private static void close(FileDescriptor descriptor) {
    if (descriptor != null)
      try {
        Os.close(descriptor);
      } catch (ErrnoException ignored) {
      }
  }

  @Override
  public void cancel() {
    stopped.set(true);
    FileDescriptor first, second;
    synchronized (this) {
      first = input;
      second = output;
      input = null;
      output = null;
    }
    close(first);
    close(second);
  }

  @Override
  public void close() {
    cancel();
    FFmpegSession value = session;
    if (value != null && !terminal(value)) FFmpegKit.cancel(value.getSessionId());
    FFmpegKitConfig.closeFFmpegPipe(inputPipe);
    FFmpegKitConfig.closeFFmpegPipe(outputPipe);
    released = true;
    if (value == null || terminal(value)) LEASE.compareAndSet(ticket, null);
  }
}
