package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import android.content.Context;
import android.content.SharedPreferences;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import java.io.*;
import java.net.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;

/** Disposable emulator + isolated actual host only; not physical speaker/latency acceptance. */
@RunWith(AndroidJUnit4.class)
public final class OwnedMasterNativeDeviceTest {
  private static final String SERVER = "http://10.0.2.2:28987", PASSWORD = "android-owned-fixture";

  private static void await(java.util.function.BooleanSupplier ready, long milliseconds)
      throws Exception {
    long deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(milliseconds);
    while (!ready.getAsBoolean()) {
      if (System.nanoTime() > deadline) fail("Native owned-master fixture timed out");
      Thread.sleep(10);
    }
  }

  private static JSONObject status() throws Exception {
    HttpURLConnection connection =
        (HttpURLConnection) new URL(SERVER + "/api/status").openConnection();
    connection.setConnectTimeout(1500);
    connection.setReadTimeout(2000);
    connection.setRequestProperty("x-amp-password", PASSWORD);
    try (InputStream input = connection.getInputStream();
        ByteArrayOutputStream bytes = new ByteArrayOutputStream()) {
      byte[] buffer = new byte[4096];
      int n;
      while ((n = input.read(buffer)) != -1) bytes.write(buffer, 0, n);
      return new JSONObject(bytes.toString("UTF-8"));
    } finally {
      connection.disconnect();
    }
  }

  private static Set<String> audioFiles(File root) {
    Set<String> names = new HashSet<>();
    File[] files = root.listFiles();
    if (files != null)
      for (File file : files) {
        if (file.isDirectory()) names.addAll(audioFiles(file));
        else if (file.getName().matches("(?i).*(\\.pcm|\\.wav|\\.mp3|\\.m4a|\\.flac)$")
            || file.getName().startsWith("fk_pipe")) names.add(file.getAbsolutePath());
      }
    return names;
  }

  @Test(timeout = 35000)
  public void generatedMasterUsesActualNativeMp3AndGuardedHttpWithoutSavedAudio() throws Exception {
    String endpoint = InstrumentationRegistry.getArguments().getString("amp_server", "");
    org.junit.Assume.assumeTrue(
        "Isolated native fixture argument is required", !endpoint.isEmpty());
    assertEquals("Never run this test against a household server", SERVER, endpoint);
    Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
    SharedPreferences settings = context.getSharedPreferences("settings", Context.MODE_PRIVATE);
    boolean previous = settings.getBoolean("owned-pcm-master-candidate", false);
    settings.edit().putBoolean("owned-pcm-master-candidate", true).commit();
    Set<String> before = audioFiles(context.getCacheDir());
    OwnedAudioOutput output = new OwnedAudioOutput();
    PhoneMasterBroadcast broadcast = null;
    List<OwnedPcmMixer.Voice> voices = new ArrayList<>();
    AtomicInteger uploads = new AtomicInteger();
    AtomicReference<byte[]> latest = new AtomicReference<>();
    AtomicReference<String> lastEvent = new AtomicReference<>("");
    try {
      JSONObject snapshot = status();
      assertFalse("Fixture must start idle", snapshot.optBoolean("running"));
      PhoneLiveSession.Identity identity =
          new PhoneLiveSession.Identity(
              snapshot.getString("server_id"),
              snapshot.getString("station_id"),
              snapshot.getString("library_id"));
      PhoneLiveHttpTransport actual = new PhoneLiveHttpTransport(SERVER, PASSWORD, identity);
      PhoneLiveSession.Transport counted =
          new PhoneLiveSession.Transport() {
            public PhoneLiveSession.Identity verify() throws IOException {
              return actual.verify();
            }

            public PhoneLiveSession.Started start(String previous) throws IOException {
              return actual.start(previous);
            }

            public long upload(String token, long sequence, byte[] data) throws IOException {
              long next = actual.upload(token, sequence, data);
              latest.set(data.clone());
              uploads.incrementAndGet();
              return next;
            }

            public void stop(String token) throws IOException {
              actual.stop(token);
            }

            public void abort() {
              actual.abort();
            }
          };
      output.mixer.owner(OwnedPcmMixer.Owner.DJ);
      output.mixer.masterGain(.5f);
      output.prepare();
      assertTrue(output.ready());
      OwnedPcmMixer.Voice a = output.mixer.voice(OwnedPcmMixer.Source.DECK_A),
          b = output.mixer.voice(OwnedPcmMixer.Source.DECK_B),
          pad = output.mixer.voice(OwnedPcmMixer.Source.SAMPLER),
          scratch = output.mixer.voice(OwnedPcmMixer.Source.SCRATCH_A);
      Collections.addAll(voices, a, b, pad, scratch);
      a.play();
      b.play();
      pad.play();
      try {
        output.mixer.voice(OwnedPcmMixer.Source.PRIVATE_CUE);
        fail("Cue cannot enter broadcast master");
      } catch (IllegalArgumentException expected) {
      }
      broadcast =
          new PhoneMasterBroadcast(
              output.mixer,
              () -> {
                try {
                  return (PhoneMasterBroadcast.Encoder)
                      Class.forName("cloud.gitwide.ampverse.FullPhoneMp3Encoder")
                          .getDeclaredConstructor(Context.class)
                          .newInstance(context);
                } catch (ReflectiveOperationException error) {
                  throw new IOException("Full native encoder unavailable", error);
                }
              },
              lastEvent::set);
      broadcast.start(new PhoneLiveSession(identity, counted), () -> true);
      for (int block = 0; block < 600; block++) {
        boolean vinyl = block >= 200 && block < 300;
        if (vinyl) scratch.play();
        else scratch.pause();
        for (int voice = 0; voice < 4; voice++) {
          if ((voice == 0 && vinyl) || (voice == 3 && !vinyl)) continue;
          OwnedPcmMixer.Voice target = voices.get(voice);
          short[] pcm = new short[960];
          double frequency = new double[] {440, 880, 1760, 1320}[voice];
          for (int frame = 0; frame < 480; frame++) {
            short value =
                (short) (3000 * Math.sin(2 * Math.PI * frequency * (block * 480 + frame) / 48000));
            pcm[frame * 2] = pcm[frame * 2 + 1] = value;
          }
          int offset = 0;
          long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(1);
          while (offset < 480) {
            offset += target.write(pcm, offset, 480 - offset, target.generation());
            if (offset < 480) {
              if (System.nanoTime() > deadline) fail("Generated source stalled");
              Thread.sleep(1);
            }
          }
        }
        Thread.sleep(10);
      }
      assertTrue("Native MP3/HTTP never delivered: " + lastEvent.get(), uploads.get() > 50);
      assertTrue(broadcast.active());
      assertTrue(status().optBoolean("on_air"));
      // An explicitly replaced session must survive the old sender's upload/cleanup.
      PhoneLiveSession replacement =
          new PhoneLiveSession(identity, new PhoneLiveHttpTransport(SERVER, PASSWORD, identity));
      replacement.begin();
      PhoneMasterBroadcast current = broadcast;
      await(() -> !current.active(), 4000);
      replacement.send(latest.get(), latest.get().length);
      assertTrue(status().optBoolean("on_air"));
      replacement.finish();
      assertTrue(replacement.stopAcknowledged());
      assertFalse(status().optBoolean("running"));
      for (String filename : audioFiles(context.getCacheDir()))
        assertTrue(
            "Native live output left saved audio or a FIFO behind: " + filename,
            before.contains(filename));
    } finally {
      if (broadcast != null) {
        broadcast.stop();
        PhoneMasterBroadcast current = broadcast;
        await(() -> !current.active(), 4000);
        broadcast.close();
      }
      for (OwnedPcmMixer.Voice voice : voices) voice.close();
      output.close();
      settings.edit().putBoolean("owned-pcm-master-candidate", previous).commit();
    }
  }
}
