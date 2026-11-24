package cloud.gitwide.ampverse;

import android.media.AudioFormat;
import android.media.AudioTrack;
import java.io.IOException;

/** One hardware clock for all owned master voices; cue tracks never use this output. */
final class OwnedAudioOutput implements AutoCloseable, OwnedPcmOutput {
  final OwnedPcmMixer mixer = new OwnedPcmMixer();
  private volatile long presentedFrames;
  private volatile IOException failure;
  private volatile boolean closed, suppressed, unsupported;

  void disqualify() {
    unsupported = true;
  }

  boolean ready() {
    return track != null && !closed && !unsupported && failure == null;
  }

  void suppress(boolean value) {
    suppressed = value;
  }

  interface Device {
    int write(short[] pcm, int offset, int count) throws IOException;

    long head();

    int bufferFrames();

    void play();

    void pause();

    void flush();

    void release();
  }

  interface DeviceFactory {
    Device create() throws IOException;
  }

  private final DeviceFactory devices;
  private final Runnable priority;
  private Device track;

  OwnedAudioOutput() {
    this(
        OwnedAudioOutput::hardware,
        () -> android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_AUDIO));
  }

  OwnedAudioOutput(DeviceFactory devices, Runnable priority) {
    this.devices = devices;
    this.priority = priority;
  }

  private Thread renderer;

  public synchronized void prepare() throws IOException {
    if (closed) throw new IOException("Owned output is closed");
    if (failure != null) throw failure;
    if (track != null) return;
    Device candidate = null;
    try {
      candidate = devices.create();
      candidate.play();
      track = candidate;
      renderer = new Thread(this::render, "amp-owned-master");
      renderer.setDaemon(true);
      renderer.start();
    } catch (RuntimeException | IOException error) {
      if (candidate != null) candidate.release();
      throw new IOException(
          "Owned PCM output is unavailable; existing playback was not replaced", error);
    }
  }

  private static Device hardware() throws IOException {
    int minimum =
        AudioTrack.getMinBufferSize(
            48000, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT);
    if (minimum <= 0 || minimum > 48000)
      throw new IOException("Bounded stereo PCM output is unavailable");
    AudioTrack output =
        new AudioTrack.Builder()
            .setAudioAttributes(
                new android.media.AudioAttributes.Builder()
                    .setUsage(android.media.AudioAttributes.USAGE_MEDIA)
                    .setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build())
            .setAudioFormat(
                new AudioFormat.Builder()
                    .setSampleRate(48000)
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                    .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                    .build())
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setBufferSizeInBytes(Math.max(minimum, 3840))
            .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
            .build();
    if (output.getState() != AudioTrack.STATE_INITIALIZED) {
      output.release();
      throw new IOException("Owned PCM output could not initialize");
    }
    output.setVolume(1f);
    return new Device() {
      public int write(short[] pcm, int offset, int count) {
        return output.write(pcm, offset, count, AudioTrack.WRITE_NON_BLOCKING);
      }

      public long head() {
        return Integer.toUnsignedLong(output.getPlaybackHeadPosition());
      }

      public int bufferFrames() {
        return output.getBufferSizeInFrames();
      }

      public void play() {
        output.play();
      }

      public void pause() {
        output.pause();
      }

      public void flush() {
        output.flush();
      }

      public void release() {
        output.release();
      }
    };
  }

  @Override
  public long presentedFrames() {
    return presentedFrames;
  }

  @Override
  public IOException failure() {
    return failure;
  }

  @Override
  public long bufferDurationUs() {
    Device output = track;
    return output == null ? 0 : output.bufferFrames() * 1000000L / 48000;
  }

  private void render() {
    priority.run();
    short[] block = new short[960];
    long wraps = 0, previous = 0;
    int offset = block.length;
    boolean hardwarePlaying = true;
    try {
      while (!closed) {
        if (suppressed) {
          if (hardwarePlaying) {
            track.pause();
            hardwarePlaying = false;
          }
          Thread.sleep(4);
          continue;
        }
        if (!hardwarePlaying) {
          track.play();
          hardwarePlaying = true;
        }
        if (offset == block.length) {
          mixer.render(block, 480);
          offset = 0;
        }
        while (offset < block.length && !closed && !suppressed) {
          int written = track.write(block, offset, block.length - offset);
          if (written < 0) throw new IOException("Owned PCM hardware output stopped");
          if (written == 0) Thread.sleep(1);
          else offset += written;
          long raw = track.head();
          if (raw < previous && previous - raw > 0x80000000L) wraps += 0x100000000L;
          previous = raw;
          presentedFrames = Math.min(mixer.frames(), wraps + raw);
        }
      }
    } catch (InterruptedException interrupted) {
      Thread.currentThread().interrupt();
    } catch (IOException | RuntimeException error) {
      failure =
          error instanceof IOException
              ? (IOException) error
              : new IOException("Owned output failed", error);
    } finally {
      Device output = track;
      if (output != null) {
        try {
          output.pause();
          output.flush();
        } catch (IllegalStateException ignored) {
        }
        output.release();
      }
    }
  }

  @Override
  public OwnedPcmMixer mixer() {
    return mixer;
  }

  @Override
  public void close() {
    closed = true;
    Thread worker = renderer;
    if (worker != null) worker.interrupt();
  }
}
