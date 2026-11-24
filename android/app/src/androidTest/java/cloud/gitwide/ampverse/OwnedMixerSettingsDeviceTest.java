package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import android.content.*;
import android.net.Uri;
import android.widget.CheckBox;
import androidx.media3.common.*;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import java.io.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import org.junit.Test;
import org.junit.runner.RunWith;

/** Generated local-input fixture on a disposable emulator, never household application data. */
@RunWith(AndroidJUnit4.class)
@androidx.annotation.OptIn(markerClass = androidx.media3.common.util.UnstableApi.class)
public final class OwnedMixerSettingsDeviceTest {
  private static void main(Runnable action) {
    InstrumentationRegistry.getInstrumentation().runOnMainSync(action);
  }

  private static void await(java.util.function.BooleanSupplier condition) throws Exception {
    long end = System.nanoTime() + TimeUnit.SECONDS.toNanos(8);
    while (!condition.getAsBoolean()) {
      if (System.nanoTime() > end) fail("Mixer settings fixture timed out");
      Thread.sleep(30);
    }
  }

  private static boolean playing() {
    AtomicBoolean value = new AtomicBoolean();
    main(() -> value.set(PlaybackService.player() != null && PlaybackService.player().isPlaying()));
    return value.get();
  }

  private static void little(DataOutputStream output, int value, int bytes) throws IOException {
    for (int i = 0; i < bytes; i++) output.writeByte(value >>> (i * 8));
  }

  private static File input(Context context) throws Exception {
    File file = new File(context.getCacheDir(), "qa-mixer-settings-input.wav");
    try (DataOutputStream out = new DataOutputStream(new FileOutputStream(file))) {
      out.writeBytes("RIFF");
      little(out, 36 + 192000, 4);
      out.writeBytes("WAVEfmt ");
      little(out, 16, 4);
      little(out, 1, 2);
      little(out, 1, 2);
      little(out, 48000, 4);
      little(out, 96000, 4);
      little(out, 2, 2);
      little(out, 16, 2);
      out.writeBytes("data");
      little(out, 192000, 4);
      byte[] silence = new byte[1920];
      for (int i = 0; i < 100; i++) out.write(silence);
    }
    return file;
  }

  @Test(timeout = 30000)
  public void betaMixerOptInWaitsForStoppedPlaybackAndNeverCutsOverLive() throws Exception {
    String endpoint = InstrumentationRegistry.getArguments().getString("amp_server", "");
    org.junit.Assume.assumeTrue("Isolated fixture argument required", !endpoint.isEmpty());
    assertEquals(
        "Never alter household settings in this fixture", "http://10.0.2.2:28987", endpoint);
    assertEquals("full", BuildConfig.EDITION);
    assertTrue(android.os.Build.VERSION.SDK_INT >= 30);
    Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
    SharedPreferences prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE);
    boolean previous = prefs.getBoolean("owned-pcm-master-candidate", false);
    prefs.edit().putBoolean("owned-pcm-master-candidate", false).commit();
    File source = input(context);
    ActivityScenario<MainActivity> home = null;
    ActivityScenario<SettingsActivity> settings = null;
    try {
      home = ActivityScenario.launch(MainActivity.class);
      AtomicBoolean ready = new AtomicBoolean();
      await(
          () -> {
            main(() -> ready.set(PlaybackService.player() != null));
            return ready.get();
          });
      main(
          () -> {
            Player player = PlaybackService.player();
            player.setMediaItem(
                new MediaItem.Builder()
                    .setMediaId("qa-mixer-settings-input")
                    .setUri(Uri.fromFile(source))
                    .build());
            player.setRepeatMode(Player.REPEAT_MODE_ONE);
            player.prepare();
            player.play();
          });
      await(OwnedMixerSettingsDeviceTest::playing);
      settings = ActivityScenario.launch(SettingsActivity.class);
      settings.onActivity(
          activity -> {
            CheckBox toggle =
                activity.findViewById(android.R.id.content).findViewWithTag("new-mixer-beta");
            assertNotNull(toggle);
            assertFalse(toggle.isEnabled());
            toggle.setChecked(true);
            assertFalse(prefs.getBoolean("owned-pcm-master-candidate", false));
          });
      main(() -> PlaybackService.player().pause());
      ActivityScenario<SettingsActivity> current = settings;
      AtomicBoolean enabled = new AtomicBoolean();
      await(
          () -> {
            current.onActivity(
                activity -> {
                  CheckBox toggle =
                      activity.findViewById(android.R.id.content).findViewWithTag("new-mixer-beta");
                  enabled.set(toggle.isEnabled());
                });
            return enabled.get();
          });
      Player original = PlaybackService.player();
      OwnedAudioOutput originalOutput = PlaybackService.ownedOutput();
      settings.onActivity(
          activity -> {
            CheckBox toggle =
                activity.findViewById(android.R.id.content).findViewWithTag("new-mixer-beta");
            toggle.setChecked(true);
            assertTrue(prefs.getBoolean("owned-pcm-master-candidate", false));
            assertSame("No live service recreation", original, PlaybackService.player());
            assertSame("No mixer replacement", originalOutput, PlaybackService.ownedOutput());
          });
      settings.close();
      settings = null;
      home.close();
      home = null;
      context.stopService(new Intent(context, PlaybackService.class));
      await(
          () -> {
            main(() -> ready.set(PlaybackService.player() == null));
            return ready.get();
          });
      home = ActivityScenario.launch(MainActivity.class);
      await(
          () -> {
            main(() -> ready.set(PlaybackService.player() != null));
            return ready.get();
          });
      main(
          () -> {
            assertNotSame(
                "Next service receives the saved setting", original, PlaybackService.player());
            assertNotNull(
                "Next service owns the opted-in master output", PlaybackService.ownedOutput());
          });
      main(
          () -> {
            PlaybackService.player().pause();
            DjEngine dj = PlaybackService.dj();
            dj.pauseAll();
            dj.assign(0, 0, source.getAbsolutePath(), "Generated one-shot", false);
          });
      await(
          () -> {
            main(
                () -> {
                  DjEngine dj = PlaybackService.existingDj();
                  if (dj.streams[0][0] == 0) dj.trigger(0, 0);
                  ready.set(dj.streams[0][0] != 0);
                });
            return ready.get();
          });
      main(
          () ->
              assertTrue(
                  "A playing pad must block mixer changes", PlaybackService.mixerSettingsBusy()));
      await(
          () -> {
            main(() -> ready.set(!PlaybackService.existingDj().samplesActive()));
            return ready.get();
          });
      main(
          () -> {
            DjEngine dj = PlaybackService.existingDj();
            assertTrue("Completed pad retains its historical control ID", dj.streams[0][0] != 0);
            assertFalse(
                "A naturally completed one-shot must release the mixer gate",
                PlaybackService.mixerSettingsBusy());
            Player cue = dj.decks[2];
            cue.setMediaItem(MediaItem.fromUri(Uri.fromFile(source)));
            cue.prepare();
            cue.play();
            assertTrue(
                "Private cue playback still blocks changing the output",
                PlaybackService.mixerSettingsBusy());
            cue.pause();
          });
    } finally {
      main(
          () -> {
            Player player = PlaybackService.player();
            if (player != null) {
              player.pause();
              player.stop();
            }
          });
      if (settings != null) settings.close();
      if (home != null) home.close();
      context.stopService(new Intent(context, PlaybackService.class));
      prefs.edit().putBoolean("owned-pcm-master-candidate", previous).commit();
      if (source.exists()) assertTrue("Generated input must be removed", source.delete());
    }
  }
}
