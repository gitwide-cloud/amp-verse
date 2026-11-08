package cloud.gitwide.ampverse;

import android.app.Activity;
import android.content.ComponentName;
import android.content.Intent;
import android.content.SharedPreferences;
import android.database.Cursor;
import android.graphics.Color;
import android.graphics.Typeface;
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
    private String module = "AMP Player";
    private Appearance appearance;
    private int queueLimit=100;
    private int bg, panel, text, muted, accent;
    private String message = "Import songs to save them on this device for offline playback.";
    private final Runnable tick = new Runnable() { public void run() { refresh(); handler.postDelayed(this, 500); } };
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        prefs = getSharedPreferences("settings", MODE_PRIVATE);
        if (state != null) { module=state.getString("module","AMP Player"); library=module.equals("AMP Library"); }
        draw();
        if (android.os.Build.VERSION.SDK_INT >= 33 && checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED)
            requestPermissions(new String[]{android.Manifest.permission.POST_NOTIFICATIONS}, 2);
    }
    @Override protected void onSaveInstanceState(Bundle state) { super.onSaveInstanceState(state); state.putString("module", module); }
    @Override public void onStart() {
        super.onStart();
        connection = new MediaController.Builder(this, new SessionToken(this, new ComponentName(this, PlaybackService.class))).buildAsync();
        final ListenableFuture<MediaController> pending = connection;
        connection.addListener(() -> {
            if (connection != pending) return;
            try {
                player = pending.get();
                syncSavedLibrary();
                player.setRepeatMode(prefs.getBoolean("repeat",false)?Player.REPEAT_MODE_ALL:Player.REPEAT_MODE_OFF);
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
        TextView view = new TextView(this); view.setText(value); view.setTextSize(size); view.setTextColor(color); view.setPadding(0, dp(3), 0, dp(3)); return view;
    }
    private LinearLayout card() {
        LinearLayout view = new LinearLayout(this); view.setOrientation(LinearLayout.VERTICAL); view.setPadding(dp(9),dp(8),dp(9),dp(8));view.setBackground(Chrome.panel(appearance,false));
        LinearLayout.LayoutParams params=new LinearLayout.LayoutParams(-1,-2);params.setMargins(0,dp(5),0,0);page.addView(view,params);return view;
    }
    private Button button(String caption,Runnable action) {
        Button view=new Button(this);view.setText(caption);view.setAllCaps(false);view.setTextSize(13);view.setMinHeight(dp(44));view.setMinimumHeight(dp(44));view.setMinWidth(0);view.setMinimumWidth(0);view.setPadding(dp(6),dp(4),dp(6),dp(4));
        view.setTextColor(appearance.classic?Color.rgb(15,18,27):text);view.setBackgroundTintList(null);view.setBackground(Chrome.button(appearance,false));view.setOnClickListener(v->action.run());return view;
    }
    private void rowButton(LinearLayout row,String caption,Runnable action) { LinearLayout.LayoutParams params=new LinearLayout.LayoutParams(0,dp(46),1);params.setMargins(dp(1),dp(2),dp(1),dp(2));row.addView(button(caption,action),params); }
    private void selectModule(String name) { module=name;library=name.equals("AMP Library");draw(); }
    private void openSettings(){startActivity(new Intent(this,SettingsActivity.class));}
    private void draw() {
        appearance=new Appearance(this);bg=appearance.background;panel=appearance.surface;text=appearance.text;muted=appearance.muted;accent=appearance.accent;
        title=null;clock=null;seek=null;play=null;status=null;
        getWindow().setStatusBarColor(bg);getWindow().setNavigationBarColor(bg);
        root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setBackground(Chrome.panel(appearance,false));
        root.setOnApplyWindowInsetsListener((view,insets)->{view.setPadding(dp(6)+insets.getSystemWindowInsetLeft(),insets.getSystemWindowInsetTop(),dp(6)+insets.getSystemWindowInsetRight(),insets.getSystemWindowInsetBottom());return insets;});setContentView(root);
        Button brand=button("ϟ AMP VERSE  ▾",()->{
            PopupMenu menu=new PopupMenu(this,root.getChildAt(0));
            for(String name:new String[]{"AMP Player","AMP Library","AMP DJ","AMP Remote","AMP Lab","AMP Cast","Settings"})menu.getMenu().add(name);
            menu.setOnMenuItemClickListener(item->{String name=item.getTitle().toString();if(name.equals("Settings"))openSettings();else selectModule(name);return true;});menu.show();
        });brand.setTag("module-menu");brand.setTextSize(21);brand.setTypeface(null,Typeface.BOLD_ITALIC);brand.setContentDescription("AMP Verse module dropdown");root.addView(brand,new LinearLayout.LayoutParams(-1,dp(44)));
        HorizontalScrollView nav=new HorizontalScrollView(this);nav.setHorizontalScrollBarEnabled(false);LinearLayout tabs=new LinearLayout(this);
        for(String name:new String[]{"AMP Player","AMP Library","AMP DJ","AMP Remote","AMP Lab","AMP Cast"}){
            Button tab=button(name.replace("AMP ",""),()->selectModule(name));tab.setBackground(Chrome.button(appearance,module.equals(name)));if(module.equals(name))tab.setTextColor(Color.BLACK);LinearLayout.LayoutParams params=new LinearLayout.LayoutParams(dp(79),dp(44));params.setMargins(1,dp(3),1,dp(3));tabs.addView(tab,params);
        }nav.addView(tabs);root.addView(nav);
        ScrollView scroll=new ScrollView(this);root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));page=new LinearLayout(this);page.setOrientation(LinearLayout.VERTICAL);scroll.addView(page);
        LinearLayout caption=card();caption.addView(label(module,18,accent));
        if(module.equals("AMP Player"))drawPlayer();
        else if(library)drawLibrary();
        else if(module.equals("AMP DJ")){
            LinearLayout dj=card();dj.addView(label("Prepared playback · early preview",17,text));
            dj.addView(label("Local sequential queue playback is available. Mixing decks, cue editing, beat sync and stems are not implemented yet.",14,muted));
            CheckBox repeat=new CheckBox(this);repeat.setText("Repeat local queue");repeat.setTextColor(text);repeat.setChecked(prefs.getBoolean("repeat",false));repeat.setOnCheckedChangeListener((v,value)->{prefs.edit().putBoolean("repeat",value).apply();if(player!=null)player.setRepeatMode(value?Player.REPEAT_MODE_ALL:Player.REPEAT_MODE_OFF);});dj.addView(repeat);
            dj.addView(button("Open player",()->selectModule("AMP Player")));drawQueue(false);
        }else{
            LinearLayout info=card();
            String detail=module.equals("AMP Remote")?"LAN control of AMP Verse desktop and Cast is planned. This Android preview does not connect or send remote commands yet.":module.equals("AMP Lab")?"AMP Lab uses the existing MP3 Lab editor foundation on desktop. Android audio editing and preparation tools are not implemented yet.":"AMP Cast is an independent radio server. Native Android station control is planned; the phone does not host a radio server in this preview.";
            info.addView(label(detail,16,text));info.addView(button("Settings",this::openSettings));
        }
        status=label(message,12,muted);page.addView(status);page.addView(label("AMP VERSE  "+BuildConfig.VERSION_NAME+" · LOCAL / OFFLINE",11,muted));refresh();
    }
    private void drawPlayer(){
        LinearLayout now=card();
        LinearLayout display=new LinearLayout(this);display.setOrientation(LinearLayout.HORIZONTAL);display.setPadding(dp(8),dp(10),dp(8),dp(10));display.setBackground(Chrome.panel(appearance,true));
        ImageView art=new ImageView(this);art.setImageResource(cloud.gitwide.ampverse.R.drawable.app_icon);art.setContentDescription("AMP Verse icon");display.addView(art,new LinearLayout.LayoutParams(dp(88),dp(88)));
        LinearLayout copy=new LinearLayout(this);copy.setOrientation(LinearLayout.VERTICAL);copy.setPadding(dp(10),0,0,0);display.addView(copy,new LinearLayout.LayoutParams(0,-2,1));
        title=label("Choose a song",19,appearance.classic?Color.rgb(163,255,37):text);title.setTypeface(null,Typeface.BOLD);title.setMaxLines(3);copy.addView(title);copy.addView(label("Local audio · saved on device",12,muted));clock=label("00:00 / --:--",20,appearance.classic?Color.rgb(163,255,37):accent);clock.setTypeface(Typeface.MONOSPACE);copy.addView(clock);now.addView(display);
        seek=new SeekBar(this);seek.setMax(1000);seek.setProgressTintList(android.content.res.ColorStateList.valueOf(accent));seek.setThumbTintList(android.content.res.ColorStateList.valueOf(accent));now.addView(seek);
        seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){dragging=true;}public void onStopTrackingTouch(SeekBar b){if(player!=null&&player.getDuration()>0)player.seekTo(player.getDuration()*b.getProgress()/1000);dragging=false;}public void onProgressChanged(SeekBar b,int v,boolean u){}});
        LinearLayout controls=new LinearLayout(this);rowButton(controls,"|◀",()->{if(player!=null)player.seekToPreviousMediaItem();});controls.getChildAt(0).setContentDescription("Previous track");
        play=button("▶",()->{if(player==null||player.getMediaItemCount()==0)return;if(player.isPlaying())player.pause();else{if(player.getPlaybackState()==Player.STATE_ENDED)player.seekToDefaultPosition();player.prepare();player.play();}refresh();});play.setContentDescription("Play or pause");controls.addView(play,new LinearLayout.LayoutParams(0,dp(46),1));
        rowButton(controls,"■",()->{if(player!=null){player.pause();player.seekToDefaultPosition();}refresh();});controls.getChildAt(2).setContentDescription("Stop playback");rowButton(controls,"▶|",()->{if(player!=null)player.seekToNextMediaItem();});controls.getChildAt(3).setContentDescription("Next track");now.addView(controls);
        LinearLayout actions=new LinearLayout(this);rowButton(actions,"Library",()->selectModule("AMP Library"));rowButton(actions,"+ Music",()->{PopupMenu menu=new PopupMenu(this,root.getChildAt(0));menu.getMenu().add("Scan folder");menu.getMenu().add("Select songs");menu.setOnMenuItemClickListener(item->{if(item.getTitle().toString().equals("Scan folder"))scanFolder();else importSongs();return true;});menu.show();});rowButton(actions,"Settings",this::openSettings);now.addView(actions);drawQueue(false);
    }
    private void drawLibrary(){
        LinearLayout files=card();files.addView(button(importing?"Importing…":"Scan music folder",this::scanFolder));files.addView(button("+ Select individual songs",this::importSongs));files.addView(label("▾ Saved on this device",17,accent));files.addView(label("    ▾ Imported ("+(player==null?0:player.getMediaItemCount())+")",15,text));drawQueue(true);
    }
    private void drawQueue(boolean editable){
        LinearLayout queue=card();queue.addView(label(editable?"AMP Library · songs":"Now Playing / Queue",16,accent));
        if(player==null||player.getMediaItemCount()==0){queue.addView(label("No songs saved yet. Import local audio to begin.",14,muted));return;}
        for(int i=0;i<Math.min(queueLimit,player.getMediaItemCount());i++){
            final int index=i;MediaItem item=player.getMediaItemAt(i);LinearLayout row=new LinearLayout(this);
            Button song=button((i+1)+". "+item.mediaMetadata.title,()->{if(player==null)return;player.seekTo(index,0);player.prepare();player.play();refresh();});song.setGravity(android.view.Gravity.START|android.view.Gravity.CENTER_VERTICAL);song.setTextSize(14);row.addView(song,new LinearLayout.LayoutParams(0,dp(46),1));
            if(editable){Button remove=button("−",()->{if(player!=null){player.removeMediaItem(index);saveQueue();draw();}});remove.setContentDescription("Remove "+item.mediaMetadata.title+" from library");row.addView(remove,new LinearLayout.LayoutParams(dp(44),dp(46)));}queue.addView(row);
        }
        if(player.getMediaItemCount()>queueLimit)queue.addView(button("Show 100 more songs",()->{queueLimit+=100;draw();}));
    }
    private void refresh() {
        if (status == null) return; status.setText(message);
        if (player == null || title == null || clock == null || seek == null || play == null) return;
        MediaItem item = player.getCurrentMediaItem(); title.setText(item == null ? "Choose a song" : item.mediaMetadata.title);
        long duration = player.getDuration(), position = player.getCurrentPosition();
        clock.setText(time(position) + " / " + (duration == C.TIME_UNSET ? "--:--" : time(duration)));
        if (!dragging) seek.setProgress(duration > 0 ? (int)(position * 1000 / duration) : 0);
        play.setText(player.isPlaying() ? "Ⅱ" : "▶");
        play.setBackground(Chrome.button(appearance,player.isPlaying()));
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
    private void scanFolder() {
        if(importing)return;
        Intent intent=new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION|Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
        startActivityForResult(intent,3);
    }
    @Override public void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if ((request != 1 && request != 3) || result != RESULT_OK || data == null) return;
        final Uri tree = request == 3 ? data.getData() : null;
        if(request==3 && tree==null)return;
        if(tree!=null)try{getContentResolver().takePersistableUriPermission(tree,Intent.FLAG_GRANT_READ_URI_PERMISSION);}catch(SecurityException ignored){}
        List<Uri> uris = new ArrayList<>();
        if (data.getClipData() != null) for (int i=0;i<data.getClipData().getItemCount();i++) uris.add(data.getClipData().getItemAt(i).getUri());
        else if (data.getData() != null) uris.add(data.getData());
        importing = true; message = tree==null?"Copying audio for offline playback…":"Scanning folder and subfolders…"; refresh();
        java.util.concurrent.ExecutorService worker = Executors.newSingleThreadExecutor();
        worker.execute(() -> {
            List<MediaItem> added = new ArrayList<>(); String error = "";
            Map<String,String> importedSources=new HashMap<>();int skipped=0;
            Set<String> savedIds=new HashSet<>();
            try{for(MediaItem saved:LibraryStore.read(this))savedIds.add(saved.mediaId);}catch(Exception failure){error=failure.getMessage();}
            if(tree!=null)try{uris.clear();uris.addAll(FolderScanner.scan(getContentResolver(),tree));}catch(Exception failure){error="Could not scan folder: "+failure.getMessage();uris.clear();}
            final android.content.SharedPreferences sources=getSharedPreferences("import-sources",MODE_PRIVATE);
            for (Uri uri : uris) {
                if(savedIds.contains(sources.getString(uri.toString(),""))){skipped++;continue;}
                File target = null;
                try {
                    String name = "Imported audio";
                    try (Cursor cursor = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) { if (cursor != null && cursor.moveToFirst()) name = cursor.getString(0); }
                    File dir = new File(getFilesDir(), "audio"); if (!dir.isDirectory() && !dir.mkdirs()) throw new IOException("Cannot create local audio folder");
                    String extension = name.lastIndexOf('.') >= 0 ? name.substring(name.lastIndexOf('.')).replaceAll("[^.a-zA-Z0-9]", "") : ".audio";
                    target = new File(dir, UUID.randomUUID() + extension);
                    try (InputStream input = getContentResolver().openInputStream(uri); OutputStream output = new FileOutputStream(target)) { if (input == null) throw new IOException("Cannot open audio"); byte[] bytes = new byte[65536]; int size; while ((size = input.read(bytes)) != -1) output.write(bytes, 0, size); }
                    added.add(LibraryStore.item(target, name));
                    importedSources.put(uri.toString(),target.getName());
                    final int count=added.size();runOnUiThread(()->{message="Saving local songs… "+count;refresh();});
                } catch (Exception failure) { if (target != null) target.delete(); error = failure.getMessage(); }
            }
            String failure = error; final int skippedCount=skipped;
            runOnUiThread(() -> {
                importing = false;
                // Persist from the current saved library if the activity controller was stopped by the picker.
                try {
                    List<MediaItem> items = LibraryStore.read(this); items.addAll(added); LibraryStore.write(this, items);
                    android.content.SharedPreferences.Editor sourceEditor=sources.edit();for(Map.Entry<String,String> entry:importedSources.entrySet())sourceEditor.putString(entry.getKey(),entry.getValue());sourceEditor.apply();
                    if (player != null) player.addMediaItems(added);
                    message = "Saved " + added.size() + " songs offline." + (skippedCount>0?" Skipped "+skippedCount+" already imported.":"") + (failure.isEmpty() ? "" : " "+failure);
                } catch (Exception saveError) { message = "Could not save imports: " + saveError.getMessage(); }
                module="AMP Library";library = true; draw();
            }); worker.shutdown();
        });
    }
}
