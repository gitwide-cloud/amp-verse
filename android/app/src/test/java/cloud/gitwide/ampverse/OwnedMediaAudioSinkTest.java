package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import androidx.media3.common.*;
import androidx.media3.exoplayer.audio.AudioSink;
import java.nio.*;
import org.junit.Test;

public final class OwnedMediaAudioSinkTest {
  static final class Output implements OwnedPcmOutput {
    final OwnedPcmMixer bus = new OwnedPcmMixer();
    long presented;

    public OwnedPcmMixer mixer() {
      return bus;
    }

    public void prepare() {}

    public long presentedFrames() {
      return presented;
    }

    public java.io.IOException failure() {
      return null;
    }

    public long bufferDurationUs() {
      return 20000;
    }

    short[] render(int frames, boolean present) {
      short[] pcm = new short[frames * 2];
      bus.render(pcm, frames);
      if (present) presented = bus.frames();
      return pcm;
    }
  }

  static Format format(int rate, int channels) {
    return new Format.Builder()
        .setSampleMimeType(MimeTypes.AUDIO_RAW)
        .setSampleRate(rate)
        .setChannelCount(channels)
        .setPcmEncoding(C.ENCODING_PCM_16BIT)
        .build();
  }

  static ByteBuffer pcm(int frames, int channels) {
    ByteBuffer data = ByteBuffer.allocate(frames * channels * 2).order(ByteOrder.LITTLE_ENDIAN);
    while (data.hasRemaining()) data.putShort((short) 1000);
    return data.flip();
  }

  @Test
  public void pauseAndHardwarePendingUseActualPresentedClock() throws Exception {
    Output out = new Output();
    OwnedMediaAudioSink sink = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.PLAYER, false);
    sink.configure(format(48000, 2), 0, null);
    sink.play();
    assertTrue(sink.handleBuffer(pcm(480, 2), 1000000, 1));
    assertEquals(1000000, sink.getCurrentPositionUs(false));
    out.render(480, false);
    assertTrue(sink.hasPendingData());
    assertEquals(1000000, sink.getCurrentPositionUs(false));
    out.presented = 480;
    assertEquals(1010000, sink.getCurrentPositionUs(false));
    assertFalse(sink.hasPendingData());
    sink.pause();
    sink.handleBuffer(pcm(480, 2), 1010000, 1);
    assertArrayEquals(new short[960], out.render(480, true));
    assertEquals(1010000, sink.getCurrentPositionUs(false));
    sink.play();
    assertEquals(1000, out.render(480, true)[0]);
    assertEquals(1020000, sink.getCurrentPositionUs(false));
    sink.release();
  }

  @Test
  public void eosWaitsForProcessorTailAndHardwarePresentation() throws Exception {
    Output out = new Output();
    OwnedMediaAudioSink sink = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.PLAYER, false);
    sink.configure(format(44100, 1), 0, null);
    sink.play();
    ByteBuffer data = pcm(4410, 1);
    while (data.hasRemaining()) {
      sink.handleBuffer(data, 2000000, 1);
      out.render(480, true);
    }
    for (int i = 0; i < 30 && !sink.isEnded(); i++) {
      sink.playToEndOfStream();
      out.render(480, true);
    }
    assertTrue(sink.isEnded());
    assertEquals(2100000, sink.getCurrentPositionUs(true), 200);
    sink.release();
  }

  @Test
  public void flushRejectsOldDecodedAudioAndResetsTimestampAnchor() throws Exception {
    Output out = new Output();
    OwnedMediaAudioSink sink = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.PLAYER, false);
    sink.configure(format(48000, 2), 0, null);
    sink.play();
    sink.handleBuffer(pcm(480, 2), 1000000, 1);
    sink.flush();
    assertEquals(AudioSink.CURRENT_POSITION_NOT_SET, sink.getCurrentPositionUs(false));
    assertArrayEquals(new short[960], out.render(480, true));
    sink.handleBuffer(pcm(480, 2), 5000000, 1);
    out.render(480, true);
    assertEquals(5010000, sink.getCurrentPositionUs(false));
    sink.release();
  }

  @Test
  public void speedCheckpointRetainsTrimmedTailAndMediaPosition() throws Exception {
    Output out = new Output();
    OwnedMediaAudioSink sink = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.PLAYER, false);
    sink.configure(format(48000, 2).buildUpon().setEncoderPadding(100).build(), 0, null);
    sink.play();
    ByteBuffer first = pcm(4800, 2);
    while (first.hasRemaining()) {
      sink.handleBuffer(first, 1000000, 1);
      out.render(480, true);
    }
    sink.setPlaybackParameters(new PlaybackParameters(2f, 1f));
    ByteBuffer second = pcm(9700, 2);
    int attempts = 0;
    while (second.hasRemaining()) {
      if (++attempts > 100) fail("Speed checkpoint stalled");
      sink.handleBuffer(second, 1100000, 1);
      out.render(480, true);
    }
    for (int i = 0; i < 100 && !sink.isEnded(); i++) {
      sink.playToEndOfStream();
      out.render(480, true);
    }
    assertTrue(sink.isEnded());
    assertEquals(1300000, sink.getCurrentPositionUs(true), 3000);
    sink.release();
  }

  @Test
  public void independentDeckPauseNeverPausesOtherDeckOrExport() throws Exception {
    Output out = new Output();
    out.bus.owner(OwnedPcmMixer.Owner.DJ);
    OwnedMediaAudioSink a = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.DECK_A, false),
        b = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.DECK_B, false);
    a.configure(format(48000, 2), 0, null);
    b.configure(format(48000, 2), 0, null);
    a.play();
    b.play();
    a.setVolume(.5f);
    b.setVolume(.25f);
    a.handleBuffer(pcm(960, 2), 1000000, 1);
    b.handleBuffer(pcm(960, 2), 2000000, 1);
    MasterPcmQueue export = new MasterPcmQueue(960);
    out.bus.fanout(export);
    short[] first = out.render(480, true);
    assertEquals(750, first[0]);
    a.pause();
    short[] second = out.render(480, true);
    assertEquals(250, second[0]);
    short[] network = new short[1920];
    assertEquals(960, export.read(network, 960));
    for (int i = 0; i < 960; i++) assertEquals(first[i], network[i]);
    for (int i = 0; i < 960; i++) assertEquals(second[i], network[i + 960]);
    assertEquals(1010000, a.getCurrentPositionUs(false));
    assertEquals(2020000, b.getCurrentPositionUs(false));
    a.release();
    b.release();
  }

  @Test
  public void explicitDiscontinuityWaitsForOldFramesBeforeNewAnchor() throws Exception {
    Output out = new Output();
    OwnedMediaAudioSink sink = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.PLAYER, false);
    sink.configure(format(48000, 2), 0, null);
    sink.play();
    sink.handleBuffer(pcm(480, 2), 1000000, 1);
    sink.handleDiscontinuity();
    ByteBuffer next = pcm(480, 2);
    assertFalse(sink.handleBuffer(next, 5000000, 1));
    assertEquals(0, next.position());
    out.render(480, true);
    assertTrue(sink.handleBuffer(next, 5000000, 1));
    out.render(480, true);
    assertEquals(5010000, sink.getCurrentPositionUs(false));
    sink.release();
  }

  @Test
  public void seekKeepsLatestPendingFormatAndTempoButResetDropsStaleFormat() throws Exception {
    Output out = new Output();
    OwnedMediaAudioSink sink = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.PLAYER, false);
    sink.configure(format(48000, 2), 0, null);
    sink.play();
    sink.handleBuffer(pcm(480, 2), 1000000, 1);
    sink.configure(format(44100, 1), 0, null);
    sink.setPlaybackParameters(new PlaybackParameters(2f, 1f));
    sink.flush();
    ByteBuffer next = pcm(4410, 1);
    for (int i = 0; i < 100 && next.hasRemaining(); i++) {
      sink.handleBuffer(next, 5000000, 1);
      out.render(480, true);
    }
    assertFalse(next.hasRemaining());
    for (int i = 0; i < 100 && !sink.isEnded(); i++) {
      sink.playToEndOfStream();
      out.render(480, true);
    }
    assertTrue(sink.isEnded());
    assertEquals(5100000, sink.getCurrentPositionUs(true), 3000);
    sink.configure(format(8000, 1), 0, null);
    sink.reset();
    sink.configure(format(48000, 2), 0, null);
    ByteBuffer afterReset = pcm(4800, 2);
    for (int i = 0; i < 100 && afterReset.hasRemaining(); i++) {
      sink.handleBuffer(afterReset, 7000000, 1);
      out.render(480, true);
    }
    for (int i = 0; i < 100 && !sink.isEnded(); i++) {
      sink.playToEndOfStream();
      out.render(480, true);
    }
    assertTrue(sink.isEnded());
    assertEquals(7100000, sink.getCurrentPositionUs(true), 3000);
    sink.release();
  }

  @Test
  public void pausedTempoChangeResumesOldQueueBeforeChangingClock() throws Exception {
    Output out = new Output();
    OwnedMediaAudioSink sink = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.PLAYER, false);
    sink.configure(format(48000, 2), 0, null);
    sink.play();
    sink.handleBuffer(pcm(480, 2), 1000000, 1);
    sink.pause();
    sink.setPlaybackParameters(new PlaybackParameters(2f, 1f));
    ByteBuffer next = pcm(4800, 2);
    assertFalse(sink.handleBuffer(next, 1010000, 1));
    assertArrayEquals(new short[960], out.render(480, true));
    assertEquals(1000000, sink.getCurrentPositionUs(false));
    sink.play();
    out.render(480, true);
    for (int i = 0; i < 100 && next.hasRemaining(); i++) {
      sink.handleBuffer(next, 1010000, 1);
      out.render(480, true);
    }
    for (int i = 0; i < 100 && !sink.isEnded(); i++) {
      sink.playToEndOfStream();
      out.render(480, true);
    }
    assertTrue(sink.isEnded());
    assertEquals(1110000, sink.getCurrentPositionUs(true), 3000);
    sink.release();
  }

  @Test
  public void unsupportedPassthroughAndChannelMapsCannotBypassMaster() throws Exception {
    Output out = new Output();
    OwnedMediaAudioSink sink = new OwnedMediaAudioSink(out, OwnedPcmMixer.Source.PLAYER, false);
    assertFalse(
        sink.supportsFormat(new Format.Builder().setSampleMimeType(MimeTypes.AUDIO_AC3).build()));
    try {
      sink.configure(format(48000, 6), 0, null);
      fail();
    } catch (AudioSink.ConfigurationException expected) {
    }
    try {
      sink.enableTunnelingV21();
      fail();
    } catch (IllegalStateException expected) {
    }
    sink.release();
  }
}
