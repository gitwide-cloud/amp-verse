package cloud.gitwide.ampverse;

import android.media.*;
import android.os.Handler;
import android.os.Looper;
import androidx.media3.common.C;
import androidx.media3.common.audio.*;
import com.google.common.collect.ImmutableList;
import java.io.*;
import java.nio.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BiConsumer;

/** Candidate sampler: bounded local decode in memory, common master voices, no audio temp files. */
@androidx.annotation.OptIn(markerClass = androidx.media3.common.util.UnstableApi.class)
final class OwnedSamplerAudio implements AutoCloseable {
  private final OwnedAudioOutput output;
  private final OwnedSamplerVoices voices;
  private final Handler main = new Handler(Looper.getMainLooper());
  private final BiConsumer<Integer, Boolean> ready;
  private final Map<Integer, AtomicBoolean> loads = new HashMap<>();
  private final ExecutorService decoding =
      Executors.newSingleThreadExecutor(
          r -> {
            Thread t = new Thread(r, "amp-pad-decode");
            t.setDaemon(true);
            return t;
          });
  private final Thread feeder;
  private volatile boolean closed;
  private int next = 1;

  OwnedSamplerAudio(OwnedAudioOutput output, BiConsumer<Integer, Boolean> ready) {
    this.output = output;
    this.ready = ready;
    voices =
        new OwnedSamplerVoices(
            output.mixer,
            () -> {
              AudioDsp dsp = new AudioDsp(false);
              try {
                dsp.configure(new AudioProcessor.AudioFormat(48000, 2, C.ENCODING_PCM_16BIT));
                dsp.flush();
              } catch (AudioProcessor.UnhandledAudioFormatException error) {
                throw new IllegalStateException(error);
              }
              ByteBuffer input = ByteBuffer.allocateDirect(1920).order(ByteOrder.LITTLE_ENDIAN);
              return (pcm, frames) -> {
                input.clear();
                for (int i = 0; i < frames * 2; i++) input.putShort(pcm[i]);
                input.flip();
                dsp.queueInput(input);
                ByteBuffer processed = dsp.getOutput().order(ByteOrder.LITTLE_ENDIAN);
                for (int i = 0; i < frames * 2; i++) pcm[i] = processed.getShort();
              };
            });
    feeder =
        new Thread(
            () -> {
              try {
                while (!closed) {
                  voices.pump(output.presentedFrames());
                  Thread.sleep(2);
                }
              } catch (InterruptedException interrupted) {
                Thread.currentThread().interrupt();
              } finally {
                voices.close();
              }
            },
            "amp-pad-pcm");
    feeder.setDaemon(true);
    feeder.start();
  }

  int load(String path) {
    final int id;
    AtomicBoolean cancel = new AtomicBoolean();
    synchronized (loads) {
      if (closed || next == Integer.MAX_VALUE) return 0;
      id = next++;
      loads.put(id, cancel);
    }
    decoding.execute(
        () -> {
          boolean success = false;
          try {
            short[] pcm = decode(path, cancel);
            synchronized (loads) {
              if (!closed && loads.get(id) == cancel && !cancel.get()) {
                voices.install(id, pcm);
                success = true;
              }
            }
          } catch (Exception unavailable) {
            /* Ready callback reports an unsupported or oversized pad. */
          }
          boolean loaded = success;
          main.post(
              () -> {
                synchronized (loads) {
                  if (!closed && loads.get(id) == cancel && !cancel.get()) ready.accept(id, loaded);
                }
              });
        });
    return id;
  }

  void unload(int id) {
    synchronized (loads) {
      AtomicBoolean cancel = loads.remove(id);
      if (cancel != null) cancel.set(true);
      voices.unload(id);
    }
  }

  int play(int id, float volume, boolean loop) {
    return closed ? 0 : voices.play(id, volume, loop);
  }

  void stop(int stream) {
    voices.stop(stream);
  }

  void volume(int stream, float value) {
    voices.volume(stream, value);
  }

  void loop(int stream, boolean value) {
    voices.loop(stream, value);
  }

  void master(float value) {
    voices.master(value);
  }

  @Override
  public void close() {
    synchronized (loads) {
      closed = true;
      for (AtomicBoolean cancel : loads.values()) cancel.set(true);
      loads.clear();
    }
    decoding.shutdownNow();
    feeder.interrupt();
    voices.close();
  }

  private static short[] decode(String path, AtomicBoolean cancelled) throws Exception {
    File file = new File(path);
    if (!file.isFile() || !file.canRead()) throw new IOException("Local pad unavailable");
    MediaExtractor source = new MediaExtractor();
    MediaCodec codec = null;
    AudioProcessingPipeline pipeline = null;
    short[] decoded = new short[OwnedSamplerVoices.MAX_FRAMES * 2];
    int count = 0;
    long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(15);
    try {
      source.setDataSource(file.getCanonicalPath());
      MediaFormat selected = null;
      for (int i = 0; i < source.getTrackCount(); i++) {
        MediaFormat format = source.getTrackFormat(i);
        String mime = format.getString(MediaFormat.KEY_MIME);
        if (mime != null && mime.startsWith("audio/")) {
          source.selectTrack(i);
          selected = format;
          break;
        }
      }
      if (selected == null) throw new IOException("Pad contains no audio");
      codec = MediaCodec.createDecoderByType(selected.getString(MediaFormat.KEY_MIME));
      codec.configure(selected, null, null, 0);
      codec.start();
      boolean inputEnded = false, outputEnded = false;
      MediaCodec.BufferInfo info = new MediaCodec.BufferInfo();
      while (!outputEnded) {
        if (cancelled.get()
            || Thread.currentThread().isInterrupted()
            || System.nanoTime() > deadline)
          throw new IOException("Pad decode cancelled or timed out");
        if (!inputEnded) {
          int index = codec.dequeueInputBuffer(10000);
          if (index >= 0) {
            ByteBuffer input = codec.getInputBuffer(index);
            if (input == null) throw new IOException("Pad decoder input unavailable");
            int size = source.readSampleData(input, 0);
            if (size < 0) {
              codec.queueInputBuffer(index, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM);
              inputEnded = true;
            } else {
              if ((source.getSampleFlags() & MediaExtractor.SAMPLE_FLAG_ENCRYPTED) != 0)
                throw new IOException("Encrypted pad is unsupported");
              codec.queueInputBuffer(index, 0, size, source.getSampleTime(), 0);
              source.advance();
            }
          }
        }
        int index = codec.dequeueOutputBuffer(info, 10000);
        if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
          if (pipeline != null) throw new IOException("Pad changed output format midstream");
          pipeline = processors(codec.getOutputFormat(), selected);
        } else if (index >= 0) {
          try {
            if (info.size > 0) {
              if (pipeline == null) pipeline = processors(codec.getOutputFormat(), selected);
              ByteBuffer pcm = codec.getOutputBuffer(index);
              if (pcm == null) throw new IOException("Pad decoder output unavailable");
              pcm.position(info.offset);
              pcm.limit(info.offset + info.size);
              while (pcm.hasRemaining()) {
                pipeline.queueInput(pcm);
                count = collect(pipeline, decoded, count);
              }
            }
            outputEnded = (info.flags & MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0;
          } finally {
            codec.releaseOutputBuffer(index, false);
          }
        }
      }
      if (pipeline == null) throw new IOException("Pad had no decoded PCM");
      pipeline.queueEndOfStream();
      while (!pipeline.isEnded()) {
        if (cancelled.get()
            || Thread.currentThread().isInterrupted()
            || System.nanoTime() > deadline) throw new IOException("Pad drain cancelled");
        count = collect(pipeline, decoded, count);
      }
      if (count < 2) throw new IOException("Pad is empty");
      return Arrays.copyOf(decoded, count);
    } finally {
      if (pipeline != null) pipeline.reset();
      if (codec != null) {
        try {
          codec.stop();
        } catch (IllegalStateException ignored) {
        }
        codec.release();
      }
      source.release();
    }
  }

  private static AudioProcessingPipeline processors(MediaFormat format, MediaFormat source)
      throws Exception {
    int encoding =
        format.containsKey(MediaFormat.KEY_PCM_ENCODING)
            ? format.getInteger(MediaFormat.KEY_PCM_ENCODING)
            : C.ENCODING_PCM_16BIT;
    if (encoding != C.ENCODING_PCM_16BIT)
      throw new IOException("Pad decoder did not provide PCM16");
    OwnedInputPcm input = new OwnedInputPcm();
    input.trim(
        source.containsKey(MediaFormat.KEY_ENCODER_DELAY)
            ? source.getInteger(MediaFormat.KEY_ENCODER_DELAY)
            : 0,
        source.containsKey(MediaFormat.KEY_ENCODER_PADDING)
            ? source.getInteger(MediaFormat.KEY_ENCODER_PADDING)
            : 0);
    SonicAudioProcessor sonic = new SonicAudioProcessor();
    sonic.setOutputSampleRateHz(48000);
    AudioProcessingPipeline pipeline = new AudioProcessingPipeline(ImmutableList.of(input, sonic));
    pipeline.configure(
        new AudioProcessor.AudioFormat(
            format.getInteger(MediaFormat.KEY_SAMPLE_RATE),
            format.getInteger(MediaFormat.KEY_CHANNEL_COUNT),
            encoding));
    pipeline.flush();
    return pipeline;
  }

  private static int collect(AudioProcessingPipeline pipeline, short[] target, int count)
      throws IOException {
    while (true) {
      ByteBuffer pcm = pipeline.getOutput().order(ByteOrder.LITTLE_ENDIAN);
      if (!pcm.hasRemaining()) return count;
      while (pcm.hasRemaining()) {
        if (count == target.length) throw new IOException("Pad exceeds ten-second memory budget");
        target[count++] = pcm.getShort();
      }
    }
  }
}
