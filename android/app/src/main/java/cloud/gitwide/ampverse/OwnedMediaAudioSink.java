package cloud.gitwide.ampverse;

import androidx.media3.common.*;
import androidx.media3.common.audio.*;
import androidx.media3.exoplayer.audio.AudioSink;
import com.google.common.collect.ImmutableList;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/** Candidate owned PCM sink. Enabled only after whole-master/device acceptance. */
@androidx.annotation.OptIn(markerClass = androidx.media3.common.util.UnstableApi.class)
final class OwnedMediaAudioSink implements AudioSink {
  private final OwnedPcmOutput output;
  private final OwnedPcmMixer.Voice voice;
  private final OwnedInputPcm input = new OwnedInputPcm();
  private final SonicAudioProcessor sonic = new SonicAudioProcessor();
  private final AudioProcessingPipeline processors;
  private final short[] transfer = new short[OwnedPcmMixer.MAX_RENDER_FRAMES * 2];
  private Listener listener;
  private Format format, pendingFormat;
  private PlaybackParameters parameters = PlaybackParameters.DEFAULT, pendingParameters;
  private AudioAttributes attributes = AudioAttributes.DEFAULT;
  private boolean playing, eos, processorEnded, released, discontinuous;
  private long anchorUs = C.TIME_UNSET, generation;
  private ByteBuffer processed = AudioProcessor.EMPTY_BUFFER;

  OwnedMediaAudioSink(OwnedPcmOutput output, OwnedPcmMixer.Source source, boolean playerFade) {
    this.output = output;
    voice = output.mixer().voice(source);
    generation = voice.generation();
    sonic.setOutputSampleRateHz(48000);
    processors =
        new AudioProcessingPipeline(ImmutableList.of(input, new AudioDsp(playerFade), sonic));
  }

  @Override
  public void setListener(Listener listener) {
    this.listener = listener;
  }

  @Override
  public boolean supportsFormat(Format value) {
    return getFormatSupport(value) != SINK_FORMAT_UNSUPPORTED;
  }

  @Override
  public int getFormatSupport(Format value) {
    return MimeTypes.AUDIO_RAW.equals(value.sampleMimeType)
            && value.pcmEncoding == C.ENCODING_PCM_16BIT
            && (value.channelCount == 1 || value.channelCount == 2)
            && value.sampleRate >= 8000
            && value.sampleRate <= 192000
            && value.encoderDelay >= 0
            && value.encoderDelay <= 48000
            && value.encoderPadding >= 0
            && value.encoderPadding <= 48000
        ? SINK_FORMAT_SUPPORTED_DIRECTLY
        : SINK_FORMAT_UNSUPPORTED;
  }

  @Override
  public void configure(Format next, int specifiedBufferSize, int[] channels)
      throws ConfigurationException {
    if (!supportsFormat(next) || channels != null)
      throw new ConfigurationException(
          "Owned master requires mapped mono/stereo PCM16; codec bypass is disabled", next);
    if (format == null) apply(next, true);
    else pendingFormat = next;
  }

  private void apply(Format next, boolean trimEncoder) throws ConfigurationException {
    try {
      input.trim(trimEncoder ? next.encoderDelay : 0, next.encoderPadding);
      sonic.setSpeed(parameters.speed);
      sonic.setPitch(parameters.pitch);
      AudioProcessor.AudioFormat result =
          processors.configure(
              new AudioProcessor.AudioFormat(
                  next.sampleRate, next.channelCount, C.ENCODING_PCM_16BIT));
      if (result.sampleRate != 48000
          || result.channelCount != 2
          || result.encoding != C.ENCODING_PCM_16BIT)
        throw new IllegalArgumentException("Owned output format changed");
      processors.flush();
      format = next;
      pendingFormat = null;
      eos = false;
      processorEnded = false;
      processed = AudioProcessor.EMPTY_BUFFER;
    } catch (AudioProcessor.UnhandledAudioFormatException | IllegalArgumentException error) {
      throw new ConfigurationException(error, next);
    }
  }

  @Override
  public boolean handleBuffer(ByteBuffer buffer, long pts, int encodedCount)
      throws InitializationException, WriteException {
    if (released || format == null) throw new WriteException(-1, format, false);
    if (output.failure() != null) throw new WriteException(-1, format, false);
    try {
      output.prepare();
    } catch (java.io.IOException error) {
      throw new InitializationException("Owned master output unavailable", 0, format, false, error);
    }
    if (pendingFormat != null
        || pendingParameters != null
        || (discontinuous && anchorUs != C.TIME_UNSET)) {
      if (!drain()) return false;
      boolean newTrack = pendingFormat != null || discontinuous;
      long nextAnchor = currentPosition();
      generation = voice.flush();
      if (pendingParameters != null) {
        parameters = pendingParameters;
        pendingParameters = null;
      }
      Format next = pendingFormat == null ? format : pendingFormat;
      try {
        apply(next, newTrack);
      } catch (ConfigurationException error) {
        throw new WriteException(-1, next, false);
      }
      anchorUs = newTrack || nextAnchor == CURRENT_POSITION_NOT_SET ? pts : nextAnchor;
      if (discontinuous && listener != null) listener.onPositionDiscontinuity();
      discontinuous = false;
      if (playing) voice.play();
    }
    if (anchorUs == C.TIME_UNSET) {
      anchorUs = Math.max(0, pts);
      if (discontinuous && listener != null) listener.onPositionDiscontinuity();
      discontinuous = false;
    }
    pump();
    if (processed.hasRemaining()) return false;
    int limit = buffer.limit();
    buffer.limit(Math.min(limit, buffer.position() + 8192));
    try {
      processors.queueInput(buffer);
    } finally {
      buffer.limit(limit);
    }
    pump();
    return !buffer.hasRemaining();
  }

  /** Pipeline/output copying runs only on this decoder thread, never the master thread. */
  private void pump() {
    if (!processed.hasRemaining())
      processed = processors.getOutput().order(ByteOrder.LITTLE_ENDIAN);
    while (processed.hasRemaining()) {
      int frames =
          Math.min(
              Math.min(processed.remaining() / 4, voice.writableFrames()), transfer.length / 2);
      if (frames == 0) return;
      int start = processed.position();
      for (int i = 0; i < frames * 2; i++) transfer[i] = processed.getShort();
      int accepted = voice.write(transfer, 0, frames, generation);
      processed.position(start + accepted * 4);
      if (accepted < frames) return;
      processed = processors.getOutput().order(ByteOrder.LITTLE_ENDIAN);
    }
  }

  private boolean drain() {
    if (!eos) {
      if (pendingParameters != null && pendingFormat == null) input.checkpoint();
      processors.queueEndOfStream();
      eos = true;
    }
    pump();
    if (!processed.hasRemaining() && processors.isEnded()) {
      processorEnded = true;
      voice.end();
    }
    return processorEnded && voice.drained(output.presentedFrames());
  }

  private long currentPosition() {
    if (anchorUs == C.TIME_UNSET) return CURRENT_POSITION_NOT_SET;
    long frames = voice.positionFrames(output.presentedFrames());
    if (frames < 0) return CURRENT_POSITION_NOT_SET;
    return anchorUs + sonic.getMediaDuration(frames * 1000000L / 48000);
  }

  @Override
  public long getCurrentPositionUs(boolean sourceEnded) {
    return currentPosition();
  }

  @Override
  public void play() {
    playing = true;
    voice.play();
  }

  @Override
  public void pause() {
    playing = false;
    voice.pause();
  }

  @Override
  public void handleDiscontinuity() {
    discontinuous = true;
  }

  @Override
  public void playToEndOfStream() {
    if (format != null) drain();
  }

  @Override
  public boolean isEnded() {
    return processorEnded && voice.drained(output.presentedFrames());
  }

  @Override
  public boolean hasPendingData() {
    return processed.hasRemaining() || voice.pending(output.presentedFrames());
  }

  @Override
  public void setPlaybackParameters(PlaybackParameters value) {
    if (!value.equals(parameters)) pendingParameters = value;
  }

  @Override
  public PlaybackParameters getPlaybackParameters() {
    return pendingParameters == null ? parameters : pendingParameters;
  }

  @Override
  public void setSkipSilenceEnabled(boolean enabled) {
    if (enabled && listener != null) listener.onSkipSilenceEnabledChanged(false);
  }

  @Override
  public boolean getSkipSilenceEnabled() {
    return false;
  }

  @Override
  public void setAudioAttributes(AudioAttributes value) {
    attributes = value;
  }

  @Override
  public AudioAttributes getAudioAttributes() {
    return attributes;
  }

  @Override
  public void setAudioSessionId(int id) {}

  @Override
  public void setAuxEffectInfo(AuxEffectInfo effect) {}

  @Override
  public long getAudioTrackBufferSizeUs() {
    return output.bufferDurationUs();
  }

  @Override
  public void enableTunnelingV21() {
    throw new IllegalStateException("Owned PCM master cannot use tunneling");
  }

  @Override
  public void disableTunneling() {}

  @Override
  public void setVolume(float volume) {
    voice.gain(Math.max(0, Math.min(1, volume)));
  }

  @Override
  public void flush() {
    generation = voice.flush();
    processors.flush();
    anchorUs = C.TIME_UNSET;
    eos = false;
    processorEnded = false;
    processed = AudioProcessor.EMPTY_BUFFER;
  }

  @Override
  public void reset() {
    flush();
    processors.reset();
    format = null;
    pendingFormat = null;
  }

  @Override
  public void release() {
    if (!released) {
      released = true;
      voice.close();
      processors.reset();
    }
  }
}
