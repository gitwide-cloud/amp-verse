package cloud.gitwide.ampverse;

import android.media.AudioFormat;
import android.media.AudioTrack;
import java.io.IOException;

/** One hardware clock for all owned master voices; cue tracks never use this output. */
final class OwnedAudioOutput implements AutoCloseable, OwnedPcmOutput {
  final OwnedPcmMixer mixer = new OwnedPcmMixer();
  private volatile long presentedFrames;
  private volatile IOException failure;
  private volatile boolean closed;
  private AudioTrack track;
  private Thread renderer;

  public synchronized void prepare() throws IOException {
    if (closed) throw new IOException("Owned output is closed");
    if (failure != null) throw failure;
    if (track != null) return;
    int minimum =
        AudioTrack.getMinBufferSize(
            48000, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT);
    if (minimum <= 0 || minimum > 48000)
      throw new IOException("Bounded stereo PCM output is unavailable");
    AudioTrack candidate = null;
    try {
      candidate =
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
      if (candidate.getState() != AudioTrack.STATE_INITIALIZED)
        throw new IOException("Owned PCM output could not initialize");
      candidate.setVolume(1f);
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
    AudioTrack output = track;
    return output == null ? 0 : output.getBufferSizeInFrames() * 1000000L / 48000;
  }

  private void render() {
    android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_AUDIO);
    short[] block = new short[960];
    long wraps = 0, previous = 0;
    try {
      while (!closed) {
        mixer.render(block, 480);
        int offset = 0;
        while (offset < block.length && !closed) {
          int written =
              track.write(block, offset, block.length - offset, AudioTrack.WRITE_NON_BLOCKING);
          if (written < 0) throw new IOException("Owned PCM hardware output stopped");
          if (written == 0) Thread.sleep(1);
          else offset += written;
          long raw = Integer.toUnsignedLong(track.getPlaybackHeadPosition());
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
      AudioTrack output = track;
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
