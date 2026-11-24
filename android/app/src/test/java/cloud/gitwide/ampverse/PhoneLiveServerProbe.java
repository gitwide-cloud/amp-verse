package cloud.gitwide.ampverse;

import java.io.*;
import java.net.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import org.json.JSONObject;

/** Native FFmpeg/protocol acceptance only; not an Android codec/device test. */
public final class PhoneLiveServerProbe {
  public static void main(String[] args) throws Exception {
    String base = args[0], password = args[1];
    HttpURLConnection status = (HttpURLConnection) new URL(base + "/api/status").openConnection();
    status.setRequestProperty("x-amp-password", password);
    JSONObject json;
    try (InputStream in = status.getInputStream()) {
      json = new JSONObject(new String(in.readAllBytes(), java.nio.charset.StandardCharsets.UTF_8));
    } finally {
      status.disconnect();
    }
    PhoneLiveSession.Identity identity =
        new PhoneLiveSession.Identity(
            json.getString("server_id"),
            json.getString("station_id"),
            json.getString("library_id"));
    PhoneLiveSession session =
        new PhoneLiveSession(identity, new PhoneLiveHttpTransport(base, password, identity));
    session.begin();
    Process encoder =
        new ProcessBuilder(
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "s16le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-i",
                "pipe:0",
                "-c:a",
                "libmp3lame",
                "-b:a",
                "192k",
                "-reservoir",
                "0",
                "-f",
                "mp3",
                "pipe:1")
            .redirectError(ProcessBuilder.Redirect.INHERIT)
            .start();
    Process decoder =
        new ProcessBuilder(
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "mp3",
                "-i",
                "pipe:0",
                "-f",
                "s16le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "pipe:1")
            .redirectError(ProcessBuilder.Redirect.INHERIT)
            .start();
    AtomicReference<Throwable> failure = new AtomicReference<>();
    AtomicLong decodedSamples = new AtomicLong(), power = new AtomicLong();
    Thread decoded =
        new Thread(
            () -> {
              try (InputStream in = decoder.getInputStream()) {
                byte[] buffer = new byte[4096];
                int n, carry = -1;
                while ((n = in.read(buffer)) != -1) {
                  for (int i = 0; i < n; i++) {
                    int value = buffer[i] & 255;
                    if (carry < 0) carry = value;
                    else {
                      short sample = (short) (carry | (value << 8));
                      power.addAndGet((long) sample * sample);
                      decodedSamples.incrementAndGet();
                      carry = -1;
                    }
                  }
                }
              } catch (Throwable t) {
                failure.set(t);
              }
            });
    HttpURLConnection listen = (HttpURLConnection) new URL(base + "/listen.mp3").openConnection();
    listen.setRequestProperty("x-amp-password", password);
    listen.setReadTimeout(10000);
    Thread listener =
        new Thread(
            () -> {
              try (InputStream in = listen.getInputStream();
                  OutputStream out = decoder.getOutputStream()) {
                byte[] buffer = new byte[4096];
                int n;
                while ((n = in.read(buffer)) != -1) out.write(buffer, 0, n);
              } catch (java.net.SocketTimeoutException expected) {
                try {
                  decoder.getOutputStream().close();
                } catch (IOException ignored) {
                }
              } catch (Throwable t) {
                failure.set(t);
              }
            });
    OwnedPcmMixer mixer = new OwnedPcmMixer();
    mixer.owner(OwnedPcmMixer.Owner.DJ);
    mixer.masterGain(.5f);
    OwnedPcmMixer.Voice a = mixer.voice(OwnedPcmMixer.Source.DECK_A),
        b = mixer.voice(OwnedPcmMixer.Source.DECK_B),
        pad = mixer.voice(OwnedPcmMixer.Source.SAMPLER),
        scratch = mixer.voice(OwnedPcmMixer.Source.SCRATCH_A);
    a.play();
    b.play();
    pad.play();
    MasterPcmQueue queue = new MasterPcmQueue(1920);
    mixer.fanout(queue);
    Thread source =
        new Thread(
            () -> {
              try (OutputStream out = encoder.getOutputStream()) {
                short[] local = new short[960], network = new short[960];
                byte[] bytes = new byte[1920];
                for (int block = 0; block < 600; block++) {
                  short[] one = new short[960],
                      two = new short[960],
                      sample = new short[960],
                      vinyl = new short[960];
                  for (int i = 0; i < 480; i++) {
                    int at = block * 480 + i;
                    for (int channel = 0; channel < 2; channel++) {
                      one[i * 2 + channel] =
                          (short) (4000 * Math.sin(at * 2 * Math.PI * 440 / 48000));
                      two[i * 2 + channel] =
                          (short) (4000 * Math.sin(at * 2 * Math.PI * 880 / 48000));
                      sample[i * 2 + channel] =
                          (short) (1000 * Math.sin(at * 2 * Math.PI * 1760 / 48000));
                      vinyl[i * 2 + channel] =
                          (short) (3000 * Math.sin(at * 2 * Math.PI * 1320 / 48000));
                    }
                  }
                  a.write(one, 0, 480, a.generation());
                  b.write(two, 0, 480, b.generation());
                  pad.write(sample, 0, 480, pad.generation());
                  if (block >= 200 && block < 300) {
                    scratch.play();
                    scratch.write(vinyl, 0, 480, scratch.generation());
                  } else scratch.pause();
                  mixer.render(local, 480);
                  if (queue.read(network, 480) != 480 || !java.util.Arrays.equals(local, network))
                    throw new AssertionError("Local/export mismatch");
                  for (int i = 0; i < network.length; i++) {
                    bytes[i * 2] = (byte) network[i];
                    bytes[i * 2 + 1] = (byte) (network[i] >> 8);
                  }
                  out.write(bytes);
                  Thread.sleep(5);
                }
              } catch (Throwable t) {
                failure.set(t);
              }
            });
    decoded.start();
    listener.start();
    source.start();
    byte[] lastPacket = null;
    try (InputStream encoded = encoder.getInputStream()) {
      byte[] packet = new byte[4096];
      int n;
      while ((n = encoded.read(packet)) != -1) {
        session.send(packet, n);
        lastPacket = java.util.Arrays.copyOf(packet, n);
      }
    }
    source.join(10000);
    if (source.isAlive()) throw new AssertionError("Source stalled");
    if (!encoder.waitFor(5, TimeUnit.SECONDS) || encoder.exitValue() != 0)
      throw new AssertionError("Encoder failed");
    session.finish();
    listener.join(5000);
    listen.disconnect();
    if (listener.isAlive()) throw new AssertionError("Listener did not stop");
    if (!decoder.waitFor(5, TimeUnit.SECONDS)) throw new AssertionError("Decode stalled");
    decoded.join(2000);
    session.finish();
    if (failure.get() != null) throw new AssertionError("Pipeline failed", failure.get());
    double seconds = decodedSamples.get() / 96000.0,
        rms = Math.sqrt(power.get() / (double) Math.max(1, decodedSamples.get()));
    if (seconds < 4 || seconds > 7 || rms < 300 || !session.stopAcknowledged())
      throw new AssertionError("Invalid decoded stream duration/RMS/stop " + seconds + "/" + rms);
    PhoneLiveSession oldOwner =
        new PhoneLiveSession(identity, new PhoneLiveHttpTransport(base, password, identity));
    oldOwner.begin();
    PhoneLiveSession replacement =
        new PhoneLiveSession(identity, new PhoneLiveHttpTransport(base, password, identity));
    replacement.begin();
    try {
      oldOwner.send(lastPacket, lastPacket.length);
      throw new AssertionError("Replaced owner uploaded");
    } catch (PhoneLiveSession.Rejected expected) {
    }
    oldOwner.finish();
    if (oldOwner.stopAcknowledged()) throw new AssertionError("Stale owner stopped replacement");
    replacement.send(lastPacket, lastPacket.length);
    replacement.finish();
    if (!replacement.stopAcknowledged()) throw new AssertionError("Replacement not stopped");
    System.out.println(
        "PASS actual stale-owner upload/stop rejection preserves replacement; PASS generated owned"
            + " DJ PCM, scratch + sampler, exact local/export parity → native MP3 → actual selected"
            + " server → decoded PCM; seconds="
            + seconds
            + ", RMS="
            + rms
            + ", uploads="
            + session.nextSequence()
            + "; no Android/device acceptance claimed");
  }
}
