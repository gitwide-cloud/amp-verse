package cloud.gitwide.ampverse;

import android.app.Activity;
import android.content.ComponentName;
import android.content.Intent;
import android.content.SharedPreferences;
import android.database.Cursor;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.OpenableColumns;
import android.view.View;
import android.widget.*;
import androidx.media3.common.C;
import androidx.media3.common.MediaItem;
import androidx.media3.common.PlaybackException;
import androidx.media3.common.Player;
import androidx.media3.session.MediaController;
import androidx.media3.session.SessionToken;
import com.google.common.util.concurrent.ListenableFuture;
import java.io.*;
import java.util.*;
import java.util.concurrent.Executors;

public final class MainActivity extends Activity {
    private MediaController player;
    private ListenableFuture<MediaController> connection;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private SharedPreferences prefs;
    private LinearLayout root, page;
    private TextView title, clock, status;
    private SeekBar seek;
    private Button play;
    private boolean library, dragging, importing;
    private int bg, panel, text, muted, accent;
    private String message = "Import songs to save them on this device for offline playback.";
    private final Runnable tick = new Runnable() { public void run() { refresh(); handler.postDelayed(this, 500); } };
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        prefs = getSharedPreferences("settings", MODE_PRIVATE);
        if (state != null) library = state.getBoolean("library");
        draw();
        if (android.os.Build.VERSION.SDK_INT >= 33 && checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED)
            requestPermissions(new String[]{android.Manifest.permission.POST_NOTIFICATIONS}, 2);
    }
    @Override protected void onSaveInstanceState(Bundle state) { super.onSaveInstanceState(state); state.putBoolean("library", library); }
    @Override public void onStart() {
        super.onStart();
        connection = new MediaController.Builder(this, new SessionToken(this, new ComponentName(this, PlaybackService.class))).buildAsync();
        final ListenableFuture<MediaController> pending = connection;
        connection.addListener(() -> {
            if (connection != pending) return;
            try {
                player = pending.get();
                syncSavedLibrary();
                player.addListener(new Player.Listener() {
                    @Override public void onPlayerError(PlaybackException error) { message = "Cannot play this file: " + error.getErrorCodeName(); refresh(); }
                    @Override public void onMediaItemTransition(MediaItem item, int reason) { refresh(); }
                });
                draw();
            } catch (Exception error) { message = "Playback service unavailable: " + error.getMessage(); refresh(); }
        }, getMainExecutor());
        handler.post(tick);
    }
    @Override public void onStop() {
        handler.removeCallbacks(tick);
        if (connection != null) MediaController.releaseFuture(connection);
        connection = null;
        player = null;
        super.onStop();
    }
    private int dp(int n) { return (int)(n * getResources().getDisplayMetrics().density); }
    private TextView label(String value, int size, int color) {
        TextView view = new TextView(this); view.setText(value); view.setTextSize(size); view.setTextColor(color); view.setPadding(0, dp(5), 0, dp(5)); return view;
    }
    private GradientDrawable box(int color) {
        GradientDrawable drawable = new GradientDrawable(); drawable.setColor(color); drawable.setCornerRadius(dp(prefs.getString("theme", "classic").equals("classic") ? 3 : 14)); drawable.setStroke(dp(1), muted); return drawable;
    }
    private LinearLayout card() {
        LinearLayout view = new LinearLayout(this); view.setOrientation(LinearLayout.VERTICAL); view.setPadding(dp(14), dp(12), dp(14), dp(12)); view.setBackground(box(panel));
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(-1, -2); params.setMargins(0, dp(9), 0, dp(5)); page.addView(view, params); return view;
    }
    private Button button(String caption, Runnable action) {
        Button button = new Button(this); button.setText(caption); button.setTextColor(text); button.setTextSize(14); button.setAllCaps(false); button.setBackgroundTintList(android.content.res.ColorStateList.valueOf(panel)); button.setOnClickListener(v -> action.run()); return button;
    }
    private void rowButton(LinearLayout row, String caption, Runnable action) { row.addView(button(caption, action), new LinearLayout.LayoutParams(0, dp(54), 1)); }
    private void draw() {
        String theme = prefs.getString("theme", "classic");
        boolean light = theme.equals("modern-light"), classic = theme.equals("classic");
        bg = Color.parseColor(light ? "#f1f3f9" : classic ? "#222426" : "#10141d");
        panel = Color.parseColor(light ? "#ffffff" : classic ? "#55585e" : "#202735");
        text = Color.parseColor(light ? "#202332" : "#f0eef7"); muted = Color.parseColor(light ? "#647089" : "#aaa4bd");
        accent = Color.parseColor(light ? "#7142cc" : classic ? "#ffd52b" : "#b586ff");
        getWindow().setStatusBarColor(bg); getWindow().setNavigationBarColor(bg);
        root = new LinearLayout(this); root.setOrientation(LinearLayout.VERTICAL); root.setPadding(dp(12), dp(8), dp(12), 0); root.setBackgroundColor(bg);
        // Respect system bars on edge-to-edge Android versions.
        root.setOnApplyWindowInsetsListener((view, insets) -> { view.setPadding(dp(12) + insets.getSystemWindowInsetLeft(), dp(8) + insets.getSystemWindowInsetTop(), dp(12) + insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom()); return insets; });
        setContentView(root);
        Button brand = button("ϟ AMP VERSE  ▾", () -> {
            PopupMenu menu = new PopupMenu(this, root.getChildAt(0));
            for (String name : new String[]{"Player", "Library", "Classic", "Modern Light", "Modern Dark"}) menu.getMenu().add(name);
            menu.setOnMenuItemClickListener(item -> { String name = item.getTitle().toString(); if (name.equals("Player") || name.equals("Library")) library = name.equals("Library"); else prefs.edit().putString("theme", name.equals("Classic") ? "classic" : name.equals("Modern Light") ? "modern-light" : "modern-dark").apply(); draw(); return true; }); menu.show();
        }); brand.setTextColor(accent); brand.setTextSize(23); brand.setTypeface(null, Typeface.BOLD_ITALIC); root.addView(brand);
        LinearLayout tabs = new LinearLayout(this); rowButton(tabs, "Player", () -> { library = false; draw(); }); rowButton(tabs, "Library", () -> { library = true; draw(); }); root.addView(tabs);
        ScrollView scroll = new ScrollView(this); root.addView(scroll, new LinearLayout.LayoutParams(-1, 0, 1));
        page = new LinearLayout(this); page.setOrientation(LinearLayout.VERTICAL); scroll.addView(page);
        LinearLayout now = card(); now.addView(label(library ? "AMP LIBRARY · LOCAL" : "LOCAL OFFLINE PLAYER", 13, accent));
        title = label("Choose a song", 24, classic ? Color.rgb(163,255,37) : text); title.setTypeface(null, Typeface.BOLD); now.addView(title);
        clock = label("00:00 / 00:00", 16, muted); now.addView(clock);
        seek = new SeekBar(this); seek.setMax(1000); now.addView(seek);
        seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            public void onStartTrackingTouch(SeekBar bar) { dragging = true; }
            public void onStopTrackingTouch(SeekBar bar) { if (player != null && player.getDuration() > 0) player.seekTo(player.getDuration() * bar.getProgress() / 1000); dragging = false; }
            public void onProgressChanged(SeekBar bar, int value, boolean user) { }
        });
        LinearLayout transport = new LinearLayout(this);
        rowButton(transport, "Previous", () -> { if (player != null) player.seekToPreviousMediaItem(); });
        play = button("Play", () -> { if (player == null || player.getMediaItemCount() == 0) return; if (player.isPlaying()) player.pause(); else { if (player.getPlaybackState() == Player.STATE_ENDED) player.seekToDefaultPosition(); player.prepare(); player.play(); } }); transport.addView(play, new LinearLayout.LayoutParams(0, dp(54), 1));
        rowButton(transport, "Next", () -> { if (player != null) player.seekToNextMediaItem(); }); now.addView(transport);
        now.addView(button("Stop playback", () -> { if (player != null) { player.pause(); player.seekToDefaultPosition(); } }));
        if (library) {
            LinearLayout files = card(); files.addView(button(importing ? "Importing…" : "+ Import local songs", this::importSongs));
            files.addView(label("Saved on this device / Imported", 15, accent));
            if (player != null) for (int i = 0; i < player.getMediaItemCount(); i++) {
                final int index = i; MediaItem item = player.getMediaItemAt(i);
                LinearLayout row = new LinearLayout(this); row.setGravity(android.view.Gravity.CENTER_VERTICAL);
                Button song = button((i+1) + ". " + item.mediaMetadata.title, () -> { player.seekTo(index, 0); player.prepare(); player.play(); refresh(); });
                row.addView(song, new LinearLayout.LayoutParams(0, -2, 1));
                Button remove = button("−", () -> { player.removeMediaItem(index); saveQueue(); draw(); }); remove.setContentDescription("Remove " + item.mediaMetadata.title + " from library"); row.addView(remove, new LinearLayout.LayoutParams(dp(48), dp(52))); files.addView(row);
            }
            if (player == null || player.getMediaItemCount() == 0) files.addView(label("Your library is empty. Import audio files to begin.", 15, muted));
        } else {
            LinearLayout settings = card(); CheckBox repeat = new CheckBox(this); repeat.setText("Repeat queue · sequential playback"); repeat.setTextColor(text); repeat.setChecked(prefs.getBoolean("repeat", false));
            repeat.setOnCheckedChangeListener((view, enabled) -> { prefs.edit().putBoolean("repeat", enabled).apply(); if (player != null) player.setRepeatMode(enabled ? Player.REPEAT_MODE_ALL : Player.REPEAT_MODE_OFF); }); settings.addView(repeat);
            settings.addView(button("Open Library / import songs", () -> { library = true; draw(); }));
            settings.addView(label("Playback continues through Android media controls when the screen is off. Prepared cues, stems and LAN remote control are coming later.", 13, muted));
        }
        status = label(message, 13, muted); page.addView(status); page.addView(label("AMP Verse 0.1 · Android preview", 12, muted)); refresh();
    }
    private void refresh() {
        if (status == null) return; status.setText(message);
        if (player == null) return;
        MediaItem item = player.getCurrentMediaItem(); title.setText(item == null ? "Choose a song" : item.mediaMetadata.title);
        long duration = player.getDuration(), position = player.getCurrentPosition();
        clock.setText(time(position) + " / " + (duration == C.TIME_UNSET ? "--:--" : time(duration)));
        if (!dragging) seek.setProgress(duration > 0 ? (int)(position * 1000 / duration) : 0);
        play.setText(player.isPlaying() ? "Pause" : "Play");
    }
    static String time(long millis) { return String.format(Locale.ROOT, "%02d:%02d", Math.max(0,millis)/60000, Math.max(0,millis)/1000%60); }
    private void syncSavedLibrary() {
        try {
            List<MediaItem> saved = LibraryStore.read(this);
            boolean same = saved.size() == player.getMediaItemCount();
            if (same) for (int i=0; i<saved.size(); i++) if (!saved.get(i).mediaId.equals(player.getMediaItemAt(i).mediaId)) { same=false; break; }
            if (same) return;
            String current = player.getCurrentMediaItem() == null ? "" : player.getCurrentMediaItem().mediaId;
            int index=0; for (int i=0;i<saved.size();i++) if (saved.get(i).mediaId.equals(current)) index=i;
            long position=player.getCurrentPosition(); boolean playing=player.getPlayWhenReady();
            player.setMediaItems(saved, index, position);
            if (playing && !saved.isEmpty()) { player.prepare(); player.play(); }
        } catch (Exception error) { message = "Cannot load saved library: " + error.getMessage(); }
    }
    private void saveQueue() {
        try { List<MediaItem> items = new ArrayList<>(); for (int i = 0; i < player.getMediaItemCount(); i++) items.add(player.getMediaItemAt(i)); LibraryStore.write(this, items); message = "Library saved on this device."; }
        catch (Exception error) { message = "Could not save library: " + error.getMessage(); }
    }
    private void importSongs() {
        if (importing) return;
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT).setType("audio/*").addCategory(Intent.CATEGORY_OPENABLE).putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
        startActivityForResult(intent, 1);
    }
    @Override public void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != 1 || result != RESULT_OK || data == null) return;
        List<Uri> uris = new ArrayList<>();
        if (data.getClipData() != null) for (int i=0;i<data.getClipData().getItemCount();i++) uris.add(data.getClipData().getItemAt(i).getUri());
        else if (data.getData() != null) uris.add(data.getData());
        importing = true; message = "Copying audio for offline playback…"; refresh();
        java.util.concurrent.ExecutorService worker = Executors.newSingleThreadExecutor();
        worker.execute(() -> {
            List<MediaItem> added = new ArrayList<>(); String error = "";
            for (Uri uri : uris) {
                File target = null;
                try {
                    String name = "Imported audio";
                    try (Cursor cursor = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) { if (cursor != null && cursor.moveToFirst()) name = cursor.getString(0); }
                    File dir = new File(getFilesDir(), "audio"); if (!dir.isDirectory() && !dir.mkdirs()) throw new IOException("Cannot create local audio folder");
                    String extension = name.lastIndexOf('.') >= 0 ? name.substring(name.lastIndexOf('.')).replaceAll("[^.a-zA-Z0-9]", "") : ".audio";
                    target = new File(dir, UUID.randomUUID() + extension);
                    try (InputStream input = getContentResolver().openInputStream(uri); OutputStream output = new FileOutputStream(target)) { if (input == null) throw new IOException("Cannot open audio"); byte[] bytes = new byte[65536]; int size; while ((size = input.read(bytes)) != -1) output.write(bytes, 0, size); }
                    added.add(LibraryStore.item(target, name));
                } catch (Exception failure) { if (target != null) target.delete(); error = failure.getMessage(); }
            }
            String failure = error;
            runOnUiThread(() -> {
                importing = false;
                // Persist from the current saved library if the activity controller was stopped by the picker.
                try {
                    List<MediaItem> items = LibraryStore.read(this); items.addAll(added); LibraryStore.write(this, items);
                    if (player != null) player.addMediaItems(added);
                    message = "Saved " + added.size() + " songs offline." + (failure.isEmpty() ? "" : " Some imports failed: " + failure);
                } catch (Exception saveError) { message = "Could not save imports: " + saveError.getMessage(); }
                library = true; draw();
            }); worker.shutdown();
        });
    }
}
