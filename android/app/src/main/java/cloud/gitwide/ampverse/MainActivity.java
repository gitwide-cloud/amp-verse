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

@androidx.annotation.OptIn(markerClass = androidx.media3.common.util.UnstableApi.class)
public final class MainActivity extends Activity {
    private MediaController player;
    private CastPanel castPanel;
    private DjPanel djPanel;
    private RemotePanel remotePanel;
    private int effectSide, effectPad;
    private ListenableFuture<MediaController> connection;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private SharedPreferences prefs;
    private LinearLayout root, page;
    private TextView title, clock, durationLabel, details, status;
    private ImageView artwork;
    private final List<TextView> playerRows = new ArrayList<>();
    private final List<View> transportButtons = new ArrayList<>();
    private String artworkKey = "";
    private byte[] displayedArtwork;
    private SeekBar seek;
    private Button play, shuffleButton, repeatButton;
    private boolean library, dragging, importing;
    private String module = "AMP Player";
    private Appearance appearance;
    private int queueLimit=100;
    private String musicQuery="";
    private String libraryTab="Songs";
    private int bg, panel, text, muted, accent;
    private String message = "";
    private final Runnable tick = new Runnable() { public void run() { refresh(); handler.postDelayed(this, 500); } };
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        try{CatalogStore.migrate(this);}catch(Exception error){message=error.getMessage();}
        prefs = getSharedPreferences("settings", MODE_PRIVATE);
        if (state != null) { module=state.getString("module","AMP Player"); library=module.equals("AMP Library");libraryTab=state.getString("libraryTab","Songs");effectSide=state.getInt("effectSide",0);effectPad=state.getInt("effectPad",0); }
        if(state==null&&getIntent().hasExtra("module")){module=getIntent().getStringExtra("module");library=module.equals("AMP Library");}
        draw();
    }
    @Override protected void onNewIntent(Intent intent){super.onNewIntent(intent);setIntent(intent);if(intent.hasExtra("module"))selectModule(intent.getStringExtra("module"));}
    @Override protected void onSaveInstanceState(Bundle state) { super.onSaveInstanceState(state); state.putString("module", module);state.putString("libraryTab",libraryTab);state.putInt("effectSide",effectSide);state.putInt("effectPad",effectPad); }
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
                player.setShuffleModeEnabled(prefs.getBoolean("shuffle",false));
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
        if(djPanel!=null)djPanel.engine.checkpoint();
        if(castPanel!=null){castPanel.close();castPanel=null;}
        if(remotePanel!=null){remotePanel.close();remotePanel=null;}
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
        Button view=new Button(this);view.setText(caption);view.setAllCaps(false);view.setTextSize(13);view.setMinHeight(dp(48));view.setMinimumHeight(dp(48));view.setMinWidth(0);view.setMinimumWidth(0);view.setPadding(dp(6),dp(4),dp(6),dp(4));
        view.setTextColor(Chrome.ink(appearance,false));view.setBackgroundTintList(null);view.setBackground(Chrome.button(appearance,false));view.setOnClickListener(v->action.run());return view;
    }
    private void rowButton(LinearLayout row,String caption,Runnable action) { LinearLayout.LayoutParams params=new LinearLayout.LayoutParams(0,dp(46),1);params.setMargins(dp(1),dp(2),dp(1),dp(2));row.addView(button(caption,action),params); }
    private void selectModule(String name) { if(name.equals("AMP Lab")){if(player!=null)player.pause();startActivity(new Intent(this,cloud.gitwide.ampverse.lab.MainActivity.class));return;} module=name;library=name.equals("AMP Library");draw(); }
    private void openSettings(){startActivity(new Intent(this,SettingsActivity.class));}
    private void draw() {
        djPanel=null;
        if(remotePanel!=null){remotePanel.close();remotePanel=null;}
        if(castPanel!=null){castPanel.close();castPanel=null;}
        appearance=new Appearance(this);bg=appearance.background;panel=appearance.surface;text=appearance.text;muted=appearance.muted;accent=appearance.accent;
        title=null;clock=null;durationLabel=null;details=null;artwork=null;artworkKey="";displayedArtwork=null;playerRows.clear();transportButtons.clear();seek=null;play=null;status=null;shuffleButton=null;repeatButton=null;
        getWindow().setStatusBarColor(bg);getWindow().setNavigationBarColor(bg);getWindow().getDecorView().setSystemUiVisibility(Color.red(bg)>200?View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR|View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR:0);
        root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setBackground(Chrome.panel(appearance,false));
        root.setOnApplyWindowInsetsListener((view,insets)->{view.setPadding(dp(6)+insets.getSystemWindowInsetLeft(),insets.getSystemWindowInsetTop(),dp(6)+insets.getSystemWindowInsetRight(),insets.getSystemWindowInsetBottom());return insets;});setContentView(root);
        LinearLayout header=new LinearLayout(this);header.setGravity(android.view.Gravity.CENTER_VERTICAL);
        Button brand=button("AMP VERSE · "+module.replace("AMP ",""),this::showModules);
        brand.setTag("module-menu");brand.setTextSize(17);brand.setGravity(android.view.Gravity.START|android.view.Gravity.CENTER_VERTICAL);
        brand.setBackgroundColor(Color.TRANSPARENT);brand.setTextColor(text);brand.setTypeface(null,Typeface.BOLD);brand.setContentDescription("AMP Verse module dropdown");
        header.addView(brand,new LinearLayout.LayoutParams(0,dp(48),1));
        Button more=button("More",this::showModules);more.setContentDescription("More apps and settings");header.addView(more,new LinearLayout.LayoutParams(dp(64),dp(48)));root.addView(header);
        page=new LinearLayout(this);page.setOrientation(LinearLayout.VERTICAL);
        if(module.equals("AMP Player"))root.addView(page,new LinearLayout.LayoutParams(-1,0,1));
        else {ScrollView scroll=new ScrollView(this);root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));scroll.addView(page);}
        navigation();
        if(module.equals("AMP Player"))drawPlayer();
        else if(library)drawLibrary();
        else if(module.equals("AMP DJ")){
            DjEngine engine=PlaybackService.dj();
            if(engine==null){LinearLayout waiting=card();waiting.addView(label("Connecting to audio service…",16,text));}
            else {djPanel=new DjPanel(this,appearance,engine,new DjPanel.Host(){public void effect(int side,int pad){effectSide=side;effectPad=pad;startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT).setType("audio/*").addCategory(Intent.CATEGORY_OPENABLE),70);}public void beforeMainPlay(){if(player!=null)player.pause();}public void equalizer(){MainActivity.this.equalizer();}});page.addView(djPanel);}
        }else if(module.equals("AMP Remote")){
            remotePanel=new RemotePanel(this,appearance);page.addView(remotePanel);
        }else if(module.equals("AMP Cast")){
            castPanel=new CastPanel(this,appearance,new CastPanel.Listener(){
                public void listen(String url,String name){
                    if(player==null){message="Playback service is still connecting. Try Listen again.";refresh();return;}
                    player.setMediaItem(new MediaItem.Builder().setMediaId("amp-radio").setUri(url).setMediaMetadata(new androidx.media3.common.MediaMetadata.Builder().setTitle(name).build()).build());
                    player.setRepeatMode(Player.REPEAT_MODE_OFF);player.setShuffleModeEnabled(false);player.prepare();player.play();message="Listening to "+name;refresh();
                }
                public void playlist(java.util.List<MediaItem> items){if(player==null||items.isEmpty()){message="No offline songs available.";return;}player.setMediaItems(items);player.setRepeatMode(Player.REPEAT_MODE_OFF);player.prepare();player.play();message="Playing offline station playlist";refresh();}
                public void stopListening(){if(player!=null&&isRadio()){player.stop();message="Phone listening stopped. Server station remains independent.";refresh();}}
            });page.addView(castPanel);
        }else{
            LinearLayout info=card();
            String detail=module.equals("AMP Remote")?"Connect to the desktop library, browse songs or playlists, listen on this phone, then Save offline for Cast. This Remote workflow is not implemented in this preview yet.":module.equals("AMP Lab")?"AMP Lab uses the existing MP3 Lab editor foundation on desktop. Android audio editing and preparation tools are not implemented yet.":"Choose AMP Cast from the module dropdown to connect to a running radio station.";
            info.addView(label(detail,16,text));info.addView(button("Settings",this::openSettings));
        }
        status=label(message,12,muted);if(module.equals("AMP Player")&&page.getOrientation()==LinearLayout.HORIZONTAL)root.addView(status,root.getChildCount()-1);else page.addView(status);refresh();
    }
    private void showModules(){new android.app.AlertDialog.Builder(this).setTitle("AMP Verse").setItems(new String[]{"AMP Player","AMP Library","AMP DJ","AMP Cast","AMP Remote","AMP Lab","Settings"},(d,n)->{String[] modules={"AMP Player","AMP Library","AMP DJ","AMP Cast","AMP Remote","AMP Lab","Settings"};if(n==6)openSettings();else selectModule(modules[n]);}).setNegativeButton("Close",null).show();}
    private void navigation(){LinearLayout navigation=new LinearLayout(this);navigation.setPadding(0,dp(4),0,dp(2));String[] names={"Player","Library","DJ","Cast","Remote"};for(String name:names){String destination="AMP "+name;boolean selected=module.equals(destination);Button tab=button(name,()->selectModule(destination));tab.setTextSize(12);tab.setSelected(selected);tab.setTextColor(Chrome.ink(appearance,selected));tab.setBackground(Chrome.button(appearance,selected));tab.setContentDescription("Open "+destination);tab.setTag("nav-"+name.toLowerCase(Locale.ROOT));LinearLayout.LayoutParams params=new LinearLayout.LayoutParams(0,-2,1);params.setMargins(dp(1),0,dp(1),0);navigation.addView(tab,params);}root.addView(navigation);}
    private void playerOptions(){boolean shuffle=player!=null&&player.getShuffleModeEnabled(),repeat=player!=null&&player.getRepeatMode()==Player.REPEAT_MODE_ALL;new android.app.AlertDialog.Builder(this).setTitle("Player options").setItems(new String[]{"Add music","Shuffle · "+(shuffle?"On":"Off"),"Repeat queue · "+(repeat?"On":"Off"),"Open stream URL"},(d,n)->{if(n==0)addMusic();else if(n==3)radio();else if(player!=null){if(n==1){player.setShuffleModeEnabled(!shuffle);prefs.edit().putBoolean("shuffle",!shuffle).apply();}else{player.setRepeatMode(repeat?Player.REPEAT_MODE_OFF:Player.REPEAT_MODE_ALL);prefs.edit().putBoolean("repeat",!repeat).apply();}refresh();}}).setNegativeButton("Close",null).show();}
    private void drawPlayer(){
        boolean shortScreen=getResources().getConfiguration().screenHeightDp<520;int coverSize=shortScreen?64:88;
        if(shortScreen)page.setOrientation(LinearLayout.HORIZONTAL);
        if(!shortScreen){LinearLayout spectrum=card();spectrum.setPadding(dp(6),dp(5),dp(6),dp(5));spectrum.addView(new SpectrumView(this),new LinearLayout.LayoutParams(-1,dp(40)));}
        LinearLayout now=card();if(shortScreen){LinearLayout.LayoutParams layout=(LinearLayout.LayoutParams)now.getLayoutParams();layout.width=0;layout.height=-1;layout.weight=1;}now.setPadding(dp(5),dp(5),dp(5),dp(3));
        LinearLayout display=new LinearLayout(this);display.setPadding(dp(4),dp(4),dp(4),dp(4));display.setBackground(Chrome.panel(appearance,true));
        artwork=new ImageView(this);artwork.setImageResource(R.drawable.ic_music_note);artwork.setScaleType(ImageView.ScaleType.CENTER_INSIDE);artwork.setContentDescription("Album artwork");display.addView(artwork,new LinearLayout.LayoutParams(dp(coverSize),dp(coverSize)));
        LinearLayout copy=new LinearLayout(this);copy.setOrientation(LinearLayout.VERTICAL);copy.setPadding(dp(8),0,0,0);display.addView(copy,new LinearLayout.LayoutParams(0,dp(coverSize),1));
        title=label("Choose a song",16,text);title.setTypeface(null,Typeface.BOLD);title.setMaxLines(2);title.setEllipsize(android.text.TextUtils.TruncateAt.END);copy.addView(title);
        details=label("Phone audio",12,text);details.setMaxLines(2);copy.addView(details);now.addView(display);
        seek=new SeekBar(this);seek.setMax(1000);seek.setProgressTintList(android.content.res.ColorStateList.valueOf(accent));seek.setThumbTintList(android.content.res.ColorStateList.valueOf(accent));seek.setContentDescription("Playback position");now.addView(seek,new LinearLayout.LayoutParams(-1,dp(32)));
        seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){dragging=true;}public void onStopTrackingTouch(SeekBar b){if(player!=null&&player.getDuration()>0)player.seekTo(player.getDuration()*b.getProgress()/1000);dragging=false;}public void onProgressChanged(SeekBar b,int v,boolean u){}});
        LinearLayout times=new LinearLayout(this);clock=label("00:00",12,text);durationLabel=label("--:--",12,text);durationLabel.setGravity(android.view.Gravity.END);times.addView(clock,new LinearLayout.LayoutParams(0,-2,1));times.addView(durationLabel,new LinearLayout.LayoutParams(0,-2,1));now.addView(times);
        LinearLayout controls=new LinearLayout(this);rowButton(controls,"|◀",()->{if(player!=null)player.seekToPreviousMediaItem();});controls.getChildAt(0).setContentDescription("Previous track");
        play=button("▶",()->{if(player==null||player.getMediaItemCount()==0)return;if(player.isPlaying())player.pause();else{if(player.getPlaybackState()==Player.STATE_ENDED)player.seekToDefaultPosition();player.prepare();player.play();}refresh();});play.setContentDescription("Play or pause");controls.addView(play,new LinearLayout.LayoutParams(0,dp(46),1));
        rowButton(controls,"■",()->{if(player!=null){player.pause();player.seekToDefaultPosition();}refresh();});controls.getChildAt(2).setContentDescription("Stop playback");rowButton(controls,"▶|",()->{if(player!=null)player.seekToNextMediaItem();});controls.getChildAt(3).setContentDescription("Next track");
        for(int i=0;i<controls.getChildCount();i++)transportButtons.add(controls.getChildAt(i));
        now.addView(controls);
        LinearLayout tools=new LinearLayout(this);rowButton(tools,"Equalizer",this::equalizer);rowButton(tools,"Cast",()->selectModule("AMP Cast"));rowButton(tools,"Options",this::playerOptions);now.addView(tools);
        drawPlayerQueue();
    }
    private void addMusic(){new android.app.AlertDialog.Builder(this).setTitle("Add music").setItems(new String[]{"Scan folder","Select songs"},(d,n)->{if(n==0)scanFolder();else importSongs();}).setNegativeButton("Cancel",null).show();}
    private void drawPlayerQueue(){
        LinearLayout queue=card();queue.setPadding(dp(4),dp(3),dp(4),dp(4));LinearLayout.LayoutParams layout=(LinearLayout.LayoutParams)queue.getLayoutParams();if(getResources().getConfiguration().screenHeightDp<520){layout.width=0;layout.height=-1;}else layout.height=0;layout.weight=1;
        TextView heading=label("Up next",14,Chrome.ink(appearance,false));heading.setTypeface(null,Typeface.BOLD);heading.setPadding(dp(5),dp(4),dp(5),dp(4));heading.setBackground(Chrome.button(appearance,false));queue.addView(heading);
        ScrollView scroll=new ScrollView(this);scroll.setBackground(Chrome.panel(appearance,true));queue.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));LinearLayout rows=new LinearLayout(this);rows.setOrientation(LinearLayout.VERTICAL);scroll.addView(rows);
        final boolean radio=isRadio();final List<MediaItem> items=new ArrayList<>();try{if(radio)items.addAll(LibraryStore.read(this));else if(player!=null)for(int i=0;i<player.getMediaItemCount();i++)items.add(player.getMediaItemAt(i));}catch(Exception error){message="Cannot read local library: "+error.getMessage();}
        if(items.isEmpty()){TextView empty=label("Your queue is empty",14,muted);empty.setPadding(dp(8),dp(10),dp(8),dp(10));rows.addView(empty);rows.addView(button("+ Add music",this::addMusic));}
        for(int i=0;i<Math.min(queueLimit,items.size());i++){final int index=i;MediaItem item=items.get(i);TextView song=label((i+1)+".  "+item.mediaMetadata.title,14,text);song.setSingleLine(true);song.setEllipsize(android.text.TextUtils.TruncateAt.END);song.setGravity(android.view.Gravity.CENTER_VERTICAL);song.setPadding(dp(8),0,dp(6),0);song.setContentDescription("Play "+item.mediaMetadata.title);song.setOnClickListener(v->{if(player==null)return;if(radio)player.setMediaItems(items);player.seekTo(index,0);player.prepare();player.play();refresh();});rows.addView(song,new LinearLayout.LayoutParams(-1,dp(44)));playerRows.add(song);}
        if(items.size()>queueLimit)rows.addView(button("Show 100 more songs",()->{queueLimit+=100;draw();}));
    }
    private void drawLibrary(){
        LinearLayout top=card();Button add=button(importing?"Adding music…":"+ Add music",this::addMusic);add.setEnabled(!importing);top.addView(add);
        LinearLayout tabs=new LinearLayout(this);for(String section:new String[]{"Songs","Playlists"}){boolean selected=libraryTab.equals(section);Button tab=button(section,()->{libraryTab=section;draw();});tab.setSelected(selected);tab.setTextColor(Chrome.ink(appearance,selected));tab.setBackground(Chrome.button(appearance,selected));tabs.addView(tab,new LinearLayout.LayoutParams(0,dp(48),1));}top.addView(tabs);
        if(libraryTab.equals("Songs"))drawQueue(true);
        else page.addView(new PhoneLibraryPanel(this,appearance,items->{if(player!=null&&!items.isEmpty()){player.setMediaItems(items);player.prepare();player.play();message="Playlist playing";}}));
    }
    private void drawQueue(boolean editable){
        LinearLayout queue=card();queue.addView(label(editable?"Songs":"Now Playing",16,accent));
        final boolean radio=isRadio();final List<MediaItem> localItems=new ArrayList<>();
        try{if(radio||editable)localItems.addAll(LibraryStore.read(this));else if(player!=null)for(int i=0;i<player.getMediaItemCount();i++)localItems.add(player.getMediaItemAt(i));}catch(Exception error){message="Cannot read local library: "+error.getMessage();}
        EditText search=new EditText(this);search.setSingleLine(true);search.setHint("Search your songs");search.setTextColor(text);search.setHintTextColor(muted);search.setContentDescription("Search phone music");search.setText(musicQuery);queue.addView(search);
        LinearLayout rows=new LinearLayout(this);rows.setOrientation(LinearLayout.VERTICAL);queue.addView(rows);
        Runnable filter=()->{rows.removeAllViews();drawMusicRows(rows,localItems,radio,editable,search.getText().toString().trim().toLowerCase(Locale.ROOT));};
        search.addTextChangedListener(new android.text.TextWatcher(){public void beforeTextChanged(CharSequence t,int a,int c,int d){}public void onTextChanged(CharSequence t,int a,int b,int c){musicQuery=t.toString();filter.run();}public void afterTextChanged(android.text.Editable t){}});filter.run();
    }
    private void drawMusicRows(LinearLayout rows,List<MediaItem> localItems,boolean radio,boolean editable,String query){
        int matches=0,shown=0;
        for(int i=0;i<localItems.size();i++){
            MediaItem candidate=localItems.get(i);
            String description=String.valueOf(candidate.mediaMetadata.title)+" "+String.valueOf(candidate.mediaMetadata.artist)+" "+String.valueOf(candidate.mediaMetadata.albumTitle);
            if(!description.toLowerCase(Locale.ROOT).contains(query))continue;matches++;if(shown++>=queueLimit)continue;
            final int index=i;MediaItem item=localItems.get(i);LinearLayout row=new LinearLayout(this);
            Button song=button((i+1)+". "+item.mediaMetadata.title,()->{if(player==null)return;if(radio||editable)player.setMediaItems(localItems);player.seekTo(index,0);player.prepare();player.play();draw();});song.setGravity(android.view.Gravity.START|android.view.Gravity.CENTER_VERTICAL);song.setOnLongClickListener(v->{PhoneLibraryPanel.trackDetails(this,item);return true;});song.setTextSize(14);row.addView(song,new LinearLayout.LayoutParams(0,dp(46),1));
            if(editable){Button remove=button("−",()->{if(player!=null){try{localItems.remove(index);LibraryStore.write(this,localItems);if(!radio){for(int n=player.getMediaItemCount()-1;n>=0;n--)if(player.getMediaItemAt(n).mediaId.equals(item.mediaId))player.removeMediaItem(n);}}catch(Exception error){message=error.getMessage();}draw();}});remove.setContentDescription("Remove "+item.mediaMetadata.title+" from library");row.addView(remove,new LinearLayout.LayoutParams(dp(44),dp(46)));}rows.addView(row);
        }
        if(matches>queueLimit)rows.addView(button("Show 100 more songs",()->{queueLimit+=100;draw();}));
        if(matches==0)rows.addView(label(localItems.isEmpty()?"No songs yet. Scan a music folder to begin.":"No songs match your search.",14,muted));
    }
    private void refresh() {
        if (status == null) return; status.setText(message);status.setVisibility(message.isEmpty()||(module.equals("AMP Player")&&!importing&&!message.startsWith("Cannot")&&!message.startsWith("Could not"))?View.GONE:View.VISIBLE);
        if (player == null || title == null || clock == null || seek == null || play == null) return;
        MediaItem item = player.getCurrentMediaItem(); title.setText(item == null ? "Choose a song" : item.mediaMetadata.title);
        long duration = player.getDuration(), position = player.getCurrentPosition();
        clock.setText(time(position));durationLabel.setText(duration <= 0 ? "--:--" : time(duration));
        androidx.media3.common.MediaMetadata metadata=player.getMediaMetadata();
        String artist=metadata.artist==null?"Phone audio":metadata.artist.toString();String album=metadata.albumTitle==null?"":metadata.albumTitle.toString();
        details.setText(item==null?"Add music or choose a playlist":artist+(album.isEmpty()?"":"\n"+album)+"\n"+(isRadio()?"Network stream":"Saved on this phone"));
        String key=item==null?"":item.mediaId; if(!key.equals(artworkKey)){artworkKey=key;displayedArtwork=null;artwork.setImageResource(R.drawable.ic_music_note);}
        if(metadata.artworkData!=null&&!Arrays.equals(displayedArtwork,metadata.artworkData)){displayedArtwork=metadata.artworkData;android.graphics.Bitmap bitmap=android.graphics.BitmapFactory.decodeByteArray(metadata.artworkData,0,metadata.artworkData.length);if(bitmap!=null){artwork.setScaleType(ImageView.ScaleType.CENTER_CROP);artwork.setImageBitmap(bitmap);}}
        for(int i=0;i<playerRows.size();i++){boolean selected=!isRadio()&&i==player.getCurrentMediaItemIndex();TextView row=playerRows.get(i);row.setBackgroundColor(selected?accent:Color.TRANSPARENT);row.setTextColor(selected?Color.BLACK:text);row.setSelected(selected);}
        if (!dragging) seek.setProgress(duration > 0 ? (int)(position * 1000 / duration) : 0);
        for(View transport:transportButtons){boolean ready=player.getMediaItemCount()>0;transport.setEnabled(ready);transport.setAlpha(ready?1f:.4f);}
        seek.setEnabled(duration>0&&!isRadio());
        play.setText(player.isPlaying() ? "Ⅱ" : "▶");
        play.setBackground(Chrome.button(appearance,player.isPlaying()));play.setTextColor(Chrome.ink(appearance,player.isPlaying()));
        if(shuffleButton!=null){shuffleButton.setSelected(player.getShuffleModeEnabled());shuffleButton.setBackground(Chrome.button(appearance,player.getShuffleModeEnabled()));}
        if(repeatButton!=null){boolean repeat=player.getRepeatMode()==Player.REPEAT_MODE_ALL;repeatButton.setSelected(repeat);repeatButton.setBackground(Chrome.button(appearance,repeat));}
    }
    static String time(long millis) { return String.format(Locale.ROOT, "%02d:%02d", Math.max(0,millis)/60000, Math.max(0,millis)/1000%60); }
    private void syncSavedLibrary() {
        if(isRadio() || (player!=null&&player.getMediaItemCount()>0))return;
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
    private boolean isRadio(){return player!=null&&player.getCurrentMediaItem()!=null&&player.getCurrentMediaItem().mediaId.equals("amp-radio");}
    private void radio(){
        EditText url=new EditText(this);url.setInputType(android.text.InputType.TYPE_CLASS_TEXT|android.text.InputType.TYPE_TEXT_VARIATION_URI);url.setSingleLine(true);url.setHint("https://station.example/stream.mp3");url.setText(prefs.getString("radio-url",""));
        new android.app.AlertDialog.Builder(this).setTitle("Internet radio / AMP Cast stream").setMessage("Live radio uses your network connection. For offline music and cues, open AMP Cast, connect to a Smart Station and save a playlist.").setView(url).setNegativeButton("Cancel",null).setPositiveButton("Play stream",(dialog,which)->{
            Uri uri=Uri.parse(url.getText().toString().trim());if(!("http".equals(uri.getScheme())||"https".equals(uri.getScheme()))||uri.getHost()==null){message="Enter a valid HTTP or HTTPS stream URL.";refresh();return;}
            if(player==null)return;prefs.edit().putString("radio-url",uri.toString()).apply();player.setMediaItem(new MediaItem.Builder().setMediaId("amp-radio").setUri(uri).setMediaMetadata(new androidx.media3.common.MediaMetadata.Builder().setTitle("Radio · "+uri.getHost()).build()).build());player.prepare();player.play();draw();
        }).show();
    }
    private void equalizer(){
        ScrollView scroll=new ScrollView(this);LinearLayout content=new LinearLayout(this);content.setPadding(dp(18),dp(8),dp(18),dp(8));content.setOrientation(LinearLayout.VERTICAL);scroll.addView(content);
        CheckBox enabled=new CheckBox(this);enabled.setText("Equalizer enabled");enabled.setChecked(prefs.getBoolean("eq-enabled",false));enabled.setOnCheckedChangeListener((v,value)->prefs.edit().putBoolean("eq-enabled",value).apply());content.addView(enabled);
        for(int i=0;i<AudioDsp.FREQUENCIES.length;i++){final int band=i;TextView name=new TextView(this);name.setText(AudioDsp.FREQUENCIES[i]+" Hz  /  "+prefs.getInt("eq-band-"+i,0)+" dB");content.addView(name);SeekBar gain=new SeekBar(this);gain.setMax(24);gain.setProgress(prefs.getInt("eq-band-"+i,0)+12);gain.setContentDescription(AudioDsp.FREQUENCIES[i]+" Hz gain");gain.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onProgressChanged(SeekBar b,int value,boolean user){if(user){prefs.edit().putInt("eq-band-"+band,value-12).apply();name.setText(AudioDsp.FREQUENCIES[band]+" Hz  /  "+(value-12)+" dB");}}public void onStartTrackingTouch(SeekBar b){}public void onStopTrackingTouch(SeekBar b){}});content.addView(gain);}
        new android.app.AlertDialog.Builder(this).setTitle("AMP Equalizer · 5 bands").setView(scroll).setNegativeButton("Flat / reset",(d,w)->{android.content.SharedPreferences.Editor edit=prefs.edit();for(int i=0;i<5;i++)edit.putInt("eq-band-"+i,0);edit.apply();}).setPositiveButton("Done",null).show();
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
        if(request==70){if(result==RESULT_OK&&data!=null&&data.getData()!=null)importEffect(data.getData(),effectSide,effectPad);return;}
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
            if(tree!=null)try{uris.clear();uris.addAll(FolderScanner.scan(getContentResolver(),tree));FolderScanner.catalog(this,tree);}catch(Exception failure){error="Could not scan folder: "+failure.getMessage();uris.clear();}
            final android.content.SharedPreferences sources=getSharedPreferences("import-sources",MODE_PRIVATE);
            for (Uri uri : uris) {
                if(savedIds.contains(sources.getString(uri.toString(),""))){if(tree!=null)getSharedPreferences("phone-folders",0).edit().putString(sources.getString(uri.toString(),""),android.provider.DocumentsContract.getTreeDocumentId(tree)).apply();skipped++;continue;}
                File target = null;
                try {
                    String name = "Imported audio";
                    try (Cursor cursor = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) { if (cursor != null && cursor.moveToFirst()) name = cursor.getString(0); }
                    File dir = new File(getFilesDir(), "audio"); if (!dir.isDirectory() && !dir.mkdirs()) throw new IOException("Cannot create local audio folder");
                    String extension = name.lastIndexOf('.') >= 0 ? name.substring(name.lastIndexOf('.')).replaceAll("[^.a-zA-Z0-9]", "") : ".audio";
                    target = new File(dir, UUID.randomUUID() + extension);
                    try (InputStream input = getContentResolver().openInputStream(uri); OutputStream output = new FileOutputStream(target)) { if (input == null) throw new IOException("Cannot open audio"); byte[] bytes = new byte[65536]; int size; while ((size = input.read(bytes)) != -1) output.write(bytes, 0, size); }
                    if(tree!=null){String prep=FolderScanner.preparation(getContentResolver(),tree,uri);if(prep!=null){CatalogStore.put(this,"tracks/"+target.getName()+".json",prep);}}
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
                    if(tree!=null){String folder=android.provider.DocumentsContract.getTreeDocumentId(tree);android.content.SharedPreferences.Editor folders=getSharedPreferences("phone-folders",0).edit();for(MediaItem item:added)folders.putString(item.mediaId,folder);folders.apply();}
                    android.content.SharedPreferences.Editor sourceEditor=sources.edit();for(Map.Entry<String,String> entry:importedSources.entrySet())sourceEditor.putString(entry.getKey(),entry.getValue());sourceEditor.apply();
                    if (player != null && !isRadio()) player.addMediaItems(added);
                    message = "Saved " + added.size() + " songs offline." + (skippedCount>0?" Skipped "+skippedCount+" already imported.":"") + (failure.isEmpty() ? "" : " "+failure);
                } catch (Exception saveError) { message = "Could not save imports: " + saveError.getMessage(); }
                module="AMP Library";library = true; draw();
            }); worker.shutdown();
        });
    }
    private void importEffect(Uri uri,int side,int pad){
        java.util.concurrent.ExecutorService worker=Executors.newSingleThreadExecutor();
        worker.execute(()->{File target=null;String error="",name="Effect";
            try{try(Cursor cursor=getContentResolver().query(uri,new String[]{OpenableColumns.DISPLAY_NAME},null,null,null)){if(cursor!=null&&cursor.moveToFirst())name=cursor.getString(0);}
                File folder=new File(getFilesDir(),"dj-effects");if(!folder.isDirectory()&&!folder.mkdirs())throw new IOException("Cannot create effects folder");
                target=new File(folder,UUID.randomUUID()+".audio");
                try(InputStream input=getContentResolver().openInputStream(uri);OutputStream output=new FileOutputStream(target)){if(input==null)throw new IOException("Cannot read effect");byte[] bytes=new byte[32768];int size,total=0;while((size=input.read(bytes))!=-1){total+=size;if(total>8*1024*1024)throw new IOException("Use a short sound effect under 8 MB");output.write(bytes,0,size);}}
                android.media.MediaMetadataRetriever metadata=new android.media.MediaMetadataRetriever();try{metadata.setDataSource(target.getAbsolutePath());String duration=metadata.extractMetadata(android.media.MediaMetadataRetriever.METADATA_KEY_DURATION);if(duration!=null&&Long.parseLong(duration)>5000)throw new IOException("Sampler effects must be 5 seconds or shorter. Use AMP Lab to trim this sound.");}finally{metadata.release();}
            }catch(Exception failure){error=failure.getMessage();if(target!=null)target.delete();target=null;}
            final File file=target;final String problem=error,title=name;
            runOnUiThread(()->{DjEngine engine=PlaybackService.dj();if(file!=null&&engine!=null){engine.assign(side,pad,file.getAbsolutePath(),title);if(djPanel!=null)djPanel.redraw();}else Toast.makeText(this,problem.isEmpty()?"Audio service unavailable":problem,Toast.LENGTH_LONG).show();});worker.shutdown();
        });
    }

}
