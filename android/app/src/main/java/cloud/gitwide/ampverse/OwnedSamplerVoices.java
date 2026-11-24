package cloud.gitwide.ampverse;

import java.util.*;

/** Bounded decoded pad cache; one feeder produces voices, master render never calls this class. */
final class OwnedSamplerVoices implements AutoCloseable {
  static final int MAX_CLIPS = 18, MAX_FRAMES = 48000 * 10, MAX_BYTES = 32 * 1024 * 1024;

  interface Effect {
    void apply(short[] pcm, int frames);
  }

  interface Effects {
    Effect create();
  }

  private static final class Clip {
    final short[] pcm;

    Clip(short[] pcm) {
      this.pcm = pcm;
    }
  }

  private static final class Playing {
    final int id, clip;
    final short[] pcm;
    final OwnedPcmMixer.Voice voice;
    final Effect effect;
    final short[] block = new short[960];
    final long generation;
    volatile float volume;
    volatile boolean loop, stopped;
    int cursor;
    boolean ended;

    Playing(
        int id,
        int clip,
        short[] pcm,
        OwnedPcmMixer.Voice voice,
        Effect effect,
        float volume,
        boolean loop) {
      this.id = id;
      this.clip = clip;
      this.pcm = pcm;
      this.voice = voice;
      this.effect = effect;
      this.volume = volume;
      this.loop = loop;
      generation = voice.generation();
    }
  }

  private final OwnedPcmMixer mixer;
  private final Effects effects;
  private final Map<Integer, Clip> clips = new HashMap<>();
  private volatile Playing[] playing = new Playing[0];
  private volatile float master = 1;
  private int bytes, next = 1;
  private boolean closed;

  OwnedSamplerVoices(OwnedPcmMixer mixer, Effects effects) {
    this.mixer = mixer;
    this.effects = effects;
  }

  synchronized void install(int id, short[] pcm) {
    if (closed
        || id <= 0
        || pcm == null
        || pcm.length < 2
        || pcm.length % 2 != 0
        || pcm.length > MAX_FRAMES * 2)
      throw new IllegalArgumentException("Pad audio must be stereo and at most ten seconds");
    Clip old = clips.get(id);
    int previous = old == null ? 0 : old.pcm.length * 2;
    if ((old == null && clips.size() >= MAX_CLIPS)
        || bytes - previous + (long) pcm.length * 2 > MAX_BYTES)
      throw new IllegalStateException("Pad cache budget reached");
    clips.put(id, new Clip(pcm.clone()));
    bytes = bytes - previous + pcm.length * 2;
  }

  synchronized int play(int clip, float volume, boolean loop) {
    Clip source = clips.get(clip);
    if (closed || source == null || playing.length >= MAX_CLIPS) return 0;
    if (!Float.isFinite(volume) || volume < 0 || volume > 1)
      throw new IllegalArgumentException("Invalid pad gain");
    if (next == Integer.MAX_VALUE) throw new IllegalStateException("Pad stream identity exhausted");
    OwnedPcmMixer.Voice voice = mixer.voice(OwnedPcmMixer.Source.SAMPLER, 960);
    Playing value;
    try {
      value = new Playing(next++, clip, source.pcm, voice, effects.create(), volume, loop);
    } catch (RuntimeException failure) {
      voice.close();
      throw failure;
    }
    Playing[] snapshot = Arrays.copyOf(playing, playing.length + 1);
    snapshot[snapshot.length - 1] = value;
    playing = snapshot;
    voice.gain(volume * master);
    voice.play();
    return value.id;
  }

  synchronized void stop(int stream) {
    Playing[] old = playing;
    int kept = 0;
    for (Playing value : old)
      if (value.id == stream) {
        value.stopped = true;
        value.voice.close();
      } else kept++;
    if (kept == old.length) return;
    Playing[] snapshot = new Playing[kept];
    int at = 0;
    for (Playing value : old) if (value.id != stream) snapshot[at++] = value;
    playing = snapshot;
  }

  synchronized void unload(int id) {
    for (Playing value : playing) if (value.clip == id) stop(value.id);
    Clip old = clips.remove(id);
    if (old != null) bytes -= old.pcm.length * 2;
  }

  void volume(int stream, float value) {
    if (!Float.isFinite(value) || value < 0 || value > 1)
      throw new IllegalArgumentException("Invalid pad volume");
    for (Playing active : playing) if (active.id == stream) active.volume = value;
  }

  void loop(int stream, boolean value) {
    for (Playing active : playing) if (active.id == stream) active.loop = value;
  }

  void master(float value) {
    if (!Float.isFinite(value) || value < 0 || value > 1)
      throw new IllegalArgumentException("Invalid master gain");
    master = value;
  }

  /** Exactly one worker calls pump; decoder/cache work and network remain elsewhere. */
  void pump(long presentedFrame) {
    for (Playing active : playing) {
      if (active.stopped) continue;
      if (active.ended) {
        if (active.voice.drained(presentedFrame)) stop(active.id);
        continue;
      }
      active.voice.gain(active.volume * master);
      int writable = active.voice.writableFrames();
      while (writable > 0 && !active.stopped) {
        int frames = Math.min(480, Math.min(writable, active.pcm.length / 2 - active.cursor));
        if (frames == 0) {
          if (active.loop) {
            active.cursor = 0;
            continue;
          }
          active.ended = true;
          active.voice.end();
          break;
        }
        System.arraycopy(active.pcm, active.cursor * 2, active.block, 0, frames * 2);
        active.effect.apply(active.block, frames);
        int accepted = active.voice.write(active.block, 0, frames, active.generation);
        active.cursor += accepted;
        writable -= accepted;
        if (accepted < frames) break;
      }
    }
  }

  synchronized int cachedBytes() {
    return bytes;
  }

  int streams() {
    return playing.length;
  }

  @Override
  public synchronized void close() {
    closed = true;
    for (Playing value : playing) {
      value.stopped = true;
      value.voice.close();
    }
    playing = new Playing[0];
    clips.clear();
    bytes = 0;
  }
}
