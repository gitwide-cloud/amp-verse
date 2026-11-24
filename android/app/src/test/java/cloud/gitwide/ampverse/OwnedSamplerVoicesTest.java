package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import org.junit.Test;

public final class OwnedSamplerVoicesTest {
  static OwnedSamplerVoices pads(OwnedPcmMixer mixer) {
    mixer.owner(OwnedPcmMixer.Owner.DJ);
    return new OwnedSamplerVoices(mixer, () -> (pcm, frames) -> {});
  }

  @Test
  public void oneShotEndsOnlyAfterItsLastFrameWasPresented() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedSamplerVoices pads = pads(mixer);
    short[] clip = {10, 11, 20, 21, 30, 31, 40, 41};
    pads.install(1, clip);
    clip[0] = 999;
    pads.play(1, 1, false);
    pads.pump(0);
    short[] out = new short[8];
    mixer.render(out, 4);
    assertArrayEquals(new short[] {10, 11, 20, 21, 30, 31, 40, 41}, out);
    pads.pump(0);
    assertEquals(1, pads.streams());
    pads.pump(4);
    assertEquals(0, pads.streams());
    pads.close();
    assertEquals(0, pads.cachedBytes());
  }

  @Test
  public void loopGainEffectAndMasterReachLocalAndEncoderTogether() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    mixer.owner(OwnedPcmMixer.Owner.DJ);
    OwnedSamplerVoices pads =
        new OwnedSamplerVoices(
            mixer,
            () ->
                (pcm, frames) -> {
                  for (int i = 0; i < frames * 2; i++) pcm[i] /= 2;
                });
    pads.install(1, new short[] {1000, 1000, 2000, 2000});
    pads.master(.5f);
    int stream = pads.play(1, .5f, true);
    pads.pump(0);
    MasterPcmQueue queue = new MasterPcmQueue(4);
    mixer.fanout(queue);
    short[] out = new short[8];
    mixer.render(out, 4);
    assertArrayEquals(new short[] {125, 125, 250, 250, 125, 125, 250, 250}, out);
    short[] network = new short[8];
    assertEquals(4, queue.read(network, 4));
    assertArrayEquals(out, network);
    pads.stop(stream);
    mixer.render(out, 4);
    assertArrayEquals(new short[8], out);
    pads.close();
  }

  @Test
  public void boundedBackpressureDoesNotAdvancePastUnqueuedSamples() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedSamplerVoices pads = pads(mixer);
    short[] source = new short[3000];
    for (int i = 0; i < source.length; i++) source[i] = (short) i;
    pads.install(1, source);
    pads.play(1, 1, false);
    pads.pump(0);
    pads.pump(0);
    short[] first = new short[1920];
    mixer.render(first, 960);
    for (int i = 0; i < first.length; i++) assertEquals(source[i], first[i]);
    pads.pump(960);
    short[] last = new short[1080];
    mixer.render(last, 540);
    for (int i = 0; i < last.length; i++) assertEquals(source[i + 1920], last[i]);
    pads.pump(1500);
    assertEquals(0, pads.streams());
    pads.close();
  }

  @Test
  public void cacheAndVoicesHaveFixedBudgets() {
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    OwnedSamplerVoices pads = pads(mixer);
    for (int i = 1; i <= 18; i++) pads.install(i, new short[] {1, 1});
    try {
      pads.install(19, new short[] {1, 1});
      fail();
    } catch (IllegalStateException expected) {
    }
    for (int i = 1; i <= 18; i++) assertTrue(pads.play(i, 1, true) > 0);
    assertEquals(0, pads.play(1, 1, true));
    pads.unload(1);
    assertEquals(17, pads.streams());
    assertEquals(68, pads.cachedBytes());
    pads.close();
    assertEquals(0, pads.streams());
  }
}
