package cloud.gitwide.ampverse;

import java.io.IOException;

/** Owned hardware boundary; a fake clock can qualify sink semantics without a device. */
interface OwnedPcmOutput {
  OwnedPcmMixer mixer();

  void prepare() throws IOException;

  long presentedFrames();

  IOException failure();

  long bufferDurationUs();
}
