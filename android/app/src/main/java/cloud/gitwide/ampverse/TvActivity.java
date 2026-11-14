package cloud.gitwide.ampverse;

import android.app.*;
import android.content.*;
import android.content.res.ColorStateList;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.*;
import android.net.Uri;
import android.os.*;
import android.view.*;
import android.widget.*;
import android.text.*;
import androidx.media3.common.*;
import androidx.media3.session.*;
import androidx.media3.session.MediaController;
import com.google.common.util.concurrent.ListenableFuture;
import java.io.File;
import java.util.*;
import java.util.concurrent.*;
import org.json.*;

/** Wide, D-pad-first shell. Playback and prepared metadata stay in the existing core. */
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
public final class TvActivity extends Activity {
    private final Handler handler=new Handler(Looper.getMainLooper());
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private final Set<View> styled=Collections.newSetFromMap(new WeakHashMap<>());
    private MediaController player;
    private ListenableFuture<MediaController> connection;
    private Appearance colors;
    private LinearLayout root,content,body,rail;
    private TextView heading,status,trackTitle,artist,time;
    private Button play,autoButton;
    private SeekBar seek;
    private String page="Player";
    private List<MediaItem> library=new ArrayList<>(),displayed=new ArrayList<>(),mixQueue=new ArrayList<>();
    private JSONObject mixRules=new JSONObject();
    private int generation;
    private boolean active,dragging,updating;
    private PodcastsPanel podcasts;
    private CastPanel cast;
    private RemotePanel remote;
    private final Runnable tick=new Runnable(){public void run(){refresh();if(page.equals("Settings")&&(updating||AppUpdate.busy)){message(AppUpdate.status);AppUpdate.installPending(TvActivity.this);}if(active)handler.postDelayed(this,500);}};
    static boolean television(Context context){UiModeManager mode=(UiModeManager)context.getSystemService(UI_MODE_SERVICE);return mode!=null&&mode.getCurrentModeType()==android.content.res.Configuration.UI_MODE_TYPE_TELEVISION;}
    @Override public void onCreate(Bundle state){super.onCreate(state);colors=new Appearance(this);if(state!=null)page=state.getString("page","Player");else{String module=getIntent().getStringExtra("module");if(module!=null)for(String name:new String[]{"Player","Library","Podcasts","Cast","Remote"})if(module.equals("AMP "+name))page=name;}draw();}
    @Override protected void onResume(){super.onResume();SharedPreferences prefs=getSharedPreferences("updates",0);if(prefs.getBoolean("permission_pending",false)&&getPackageManager().canRequestPackageInstalls())prefs.edit().putBoolean("permission_pending",false).putBoolean("install",true).apply();if(prefs.getBoolean("install",false))updating=true;Modules.installPending(this);}
    @Override protected void onSaveInstanceState(Bundle state){super.onSaveInstanceState(state);state.putString("page",page);}
    @Override protected void onStart(){super.onStart();active=true;show(page,false);
        connection=new MediaController.Builder(this,new SessionToken(this,new ComponentName(this,PlaybackService.class))).buildAsync();
        ListenableFuture<MediaController> pending=connection;
        pending.addListener(()->{if(connection!=pending)return;try{player=pending.get();refresh();}catch(Exception e){message("Playback connection failed. Reopen Player to retry.");}},getMainExecutor());
        reloadLibrary();handler.post(tick);
    }
    @Override protected void onStop(){active=false;generation++;handler.removeCallbacksAndMessages(null);closePanels();if(connection!=null)MediaController.releaseFuture(connection);connection=null;player=null;super.onStop();}
    @Override protected void onDestroy(){worker.shutdownNow();super.onDestroy();}
    int dp(int n){return Math.round(n*getResources().getDisplayMetrics().density);}
    private TextView text(String value,int size){TextView t=new TextView(this);t.setText(value);t.setTextSize(size);t.setTextColor(colors.text);t.setPadding(dp(4),dp(6),dp(4),dp(6));return t;}
    private Drawable tile(int fill,int edge){GradientDrawable d=new GradientDrawable();d.setColor(fill);d.setCornerRadius(dp(10));d.setStroke(dp(2),edge);return d;}
    private void focusStyle(View view){
        if(!styled.add(view))return;
        if(view.getId()==View.NO_ID)view.setId(View.generateViewId());
        if(view instanceof Button||view instanceof EditText){TextView t=(TextView)view;t.setTextSize(18);t.setMinimumHeight(dp(56));t.setPadding(dp(14),dp(8),dp(14),dp(8));
            StateListDrawable states=new StateListDrawable();states.addState(new int[]{android.R.attr.state_focused},tile(colors.surface,colors.accent));states.addState(new int[]{android.R.attr.state_pressed},tile(colors.surface,colors.accent));states.addState(new int[]{},tile(colors.surface,0xff505762));view.setBackground(states);StateListDrawable outline=new StateListDrawable();outline.addState(new int[]{android.R.attr.state_focused},tile(Color.TRANSPARENT,colors.accent));outline.addState(new int[]{},tile(Color.TRANSPARENT,Color.TRANSPARENT));view.setForeground(outline);view.setFocusable(true);
        }
    }
    private void styleTree(View view){if(view instanceof Button||view instanceof EditText)focusStyle(view);else if(view instanceof TextView&&!styled.contains(view)){styled.add(view);TextView t=(TextView)view;if(t.getTextSize()/getResources().getDisplayMetrics().scaledDensity<18)t.setTextSize(18);}if(view instanceof ViewGroup){ViewGroup g=(ViewGroup)view;for(int i=0;i<g.getChildCount();i++)styleTree(g.getChildAt(i));}}
    private Button button(String title,Runnable action){Button b=new Button(this);b.setText(title);b.setAllCaps(false);b.setTextColor(colors.text);b.setContentDescription(title);focusStyle(b);b.setOnClickListener(v->action.run());return b;}
    private void draw(){
        getWindow().setStatusBarColor(colors.background);getWindow().setNavigationBarColor(colors.background);
        root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setBackgroundColor(colors.background);root.setPadding(dp(36),dp(20),dp(36),dp(20));setContentView(root);
        LinearLayout header=new LinearLayout(this);header.setGravity(Gravity.CENTER_VERTICAL);TextView brand=text("ϟ AMP VERSE",26);brand.setTypeface(null,Typeface.BOLD);brand.setTextColor(colors.accent);header.addView(brand,new LinearLayout.LayoutParams(0,-2,1));header.addView(text("TV · "+BuildConfig.VERSION_NAME+" · "+BuildConfig.EDITION,18));root.addView(header);
        LinearLayout workspace=new LinearLayout(this);root.addView(workspace,new LinearLayout.LayoutParams(-1,0,1));
        rail=new LinearLayout(this);rail.setOrientation(LinearLayout.VERTICAL);rail.setPadding(0,dp(12),dp(18),0);ScrollView navigation=new ScrollView(this);navigation.setFillViewport(false);navigation.addView(rail,new ScrollView.LayoutParams(-1,-2));workspace.addView(navigation,new LinearLayout.LayoutParams(dp(180),-1));
        for(String name:new String[]{"Player","Library","Playlists","Podcasts","Devices","Settings"}){Button tab=button(name,()->show(name,false));tab.setTag("tv-nav-"+name);LinearLayout.LayoutParams lp=new LinearLayout.LayoutParams(-1,dp(56));lp.setMargins(0,dp(3),0,dp(3));rail.addView(tab,lp);}
        content=new LinearLayout(this);content.setOrientation(LinearLayout.VERTICAL);content.setPadding(dp(12),0,0,0);workspace.addView(content,new LinearLayout.LayoutParams(0,-1,1));
        heading=text(page,24);heading.setTag("tv-heading");content.addView(heading);body=new LinearLayout(this);body.setOrientation(LinearLayout.VERTICAL);content.addView(body,new LinearLayout.LayoutParams(-1,0,1));
        status=text("D-pad to move · OK to select · Back to Player",16);root.addView(status);
        root.getViewTreeObserver().addOnGlobalLayoutListener(()->styleTree(body));
        rail.getChildAt(0).requestFocus();
    }
    private void closePanels(){if(podcasts!=null){podcasts.close();podcasts=null;}if(cast!=null){cast.close();cast=null;}if(remote!=null){remote.close();remote=null;}}
    private void message(String value){if(status!=null)status.setText(value);}
    private void show(String destination,boolean focusContent){
        generation++;closePanels();page=destination;heading.setText(destination);body.removeAllViews();trackTitle=null;artist=null;time=null;seek=null;play=null;autoButton=null;
        for(int i=0;i<rail.getChildCount();i++){Button tab=(Button)rail.getChildAt(i);tab.setTextColor(tab.getText().toString().equals(page)?colors.accent:colors.text);}
        message("D-pad to move · OK to select · Back to Player");
        switch(page){
            case "Player":drawPlayer();break;
            case "Library":drawLibrary();break;
            case "Playlists":drawPlaylists(false);break;
            case "Offline":drawPlaylists(true);break;
            case "Devices":row(body,button("Find stations / Cast",()->show("Cast",true)),button("Control desktop / Sync playlists",()->show("Remote",true)));body.addView(text("Cast plays station audio here or on a speaker. Remote controls another AMP installation and saves playlists for this TV.",20));break;
            case "Podcasts":if(!Modules.embedded("podcasts")){module("podcasts");break;}podcasts=new PodcastsPanel(this,colors);scroll(podcasts);break;
            case "Cast":if(!Modules.embedded("cast")){module("cast");break;}cast=new CastPanel(this,colors,new CastPanel.Listener(){public void listen(String url,String name){if(player==null)return;PlaybackService.switchToPlayer();player.setMediaItem(new MediaItem.Builder().setMediaId("amp-radio").setUri(url).setMediaMetadata(new MediaMetadata.Builder().setTitle(name).build()).build());player.prepare();player.play();message("Listening here on the TV");}public void stopListening(){if(player!=null)player.pause();}public void playlist(List<MediaItem> songs){playSongs(songs,0);}});scroll(cast);break;
            case "Remote":if(!Modules.embedded("remote")){module("remote");break;}remote=new RemotePanel(this,colors);scroll(remote);break;
            default:drawSettings();
        }
        if(focusContent)body.post(()->{List<View> targets=body.getFocusables(View.FOCUS_FORWARD);if(!targets.isEmpty())targets.get(0).requestFocus();});
        refresh();
    }
    private void module(String id){body.addView(text("This TV workflow is included in AMP Verse Full.",20));body.addView(button("Open / install "+Modules.label(id),()->Modules.open(this,id)));}
    private void scroll(View child){ScrollView s=new ScrollView(this);s.setFillViewport(false);s.addView(child);body.addView(s,new LinearLayout.LayoutParams(-1,-1));}
    private void row(LinearLayout parent,Button... buttons){LinearLayout row=new LinearLayout(this);for(Button b:buttons){LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(0,dp(60),1);p.setMargins(dp(3),dp(4),dp(3),dp(4));row.addView(b,p);}parent.addView(row);}
    private androidx.media3.common.Player output(){DjEngine dj=PlaybackService.existingDj();return "dj".equals(PlaybackService.owner())&&dj!=null?dj.decks[dj.fader<.5?0:1]:player;}
    private void drawPlayer(){
        LinearLayout screen=new LinearLayout(this);screen.setOrientation(LinearLayout.VERTICAL);scroll(screen);
        LinearLayout display=new LinearLayout(this);display.setOrientation(LinearLayout.VERTICAL);display.setPadding(dp(18),dp(12),dp(18),dp(12));display.setBackground(tile(colors.surface,0xff505762));
        trackTitle=text("Choose music from Library or Playlists",28);trackTitle.setMaxLines(2);trackTitle.setEllipsize(TextUtils.TruncateAt.END);display.addView(trackTitle);
        artist=text("Playback on this TV",20);display.addView(artist);SpectrumView spectrum=new SpectrumView(this);display.addView(spectrum,new LinearLayout.LayoutParams(-1,dp(44)));
        seek=new SeekBar(this);seek.setMax(1000);seek.setContentDescription("Playback position; left or right to seek");seek.setProgressTintList(ColorStateList.valueOf(colors.accent));seek.setThumbTintList(ColorStateList.valueOf(colors.accent));seek.setMinimumHeight(dp(48));
        seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar s){dragging=true;}public void onStopTrackingTouch(SeekBar s){dragging=false;}public void onProgressChanged(SeekBar s,int value,boolean user){androidx.media3.common.Player p=output();if(user&&p!=null&&p.getDuration()>0&&p.isCurrentMediaItemSeekable())p.seekTo(p.getDuration()*value/1000);}});display.addView(seek);time=text("00:00 / --:--",18);display.addView(time);screen.addView(display);
        play=button("Play",this::togglePlay);row(screen,button("Previous",()->{if(player!=null){PlaybackService.switchToPlayer();player.seekToPreviousMediaItem();}}),play,button("Next",()->{DjEngine d=PlaybackService.existingDj();if("dj".equals(PlaybackService.owner())&&d!=null)d.mixNext();else if(player!=null)player.seekToNextMediaItem();}));
        autoButton=button("Auto Mix · Off",this::toggleAuto);row(screen,autoButton,button("Next mix",()->{DjEngine d=PlaybackService.existingDj();if(d!=null&&d.auto)d.mixNext();else message("Start Auto Mix from a playlist first.");}),button("Fade time",this::fadeTime));
        screen.addView(text("Simple DJ · prepared beat matching and crossfades. No analysis runs on the TV.",18));
    }
    private void togglePlay(){androidx.media3.common.Player p=output();if(p==null){message("Connecting to playback…");return;}DjEngine d=PlaybackService.existingDj();if("dj".equals(PlaybackService.owner())&&d!=null){if(d.active(0)||d.active(1)){d.pauseAll();message("Mix paused; Auto Mix is off.");}else d.play(d.fader<.5?0:1);}else if(p.isPlaying())p.pause();else if(p.getMediaItemCount()>0){if(p.getPlaybackState()==androidx.media3.common.Player.STATE_ENDED)p.seekToDefaultPosition();p.prepare();p.play();}else show("Library",true);refresh();}
    private void toggleAuto(){if(!Modules.embedded("dj")){module("dj");return;}if(player==null){message("Connecting to playback…");return;}DjEngine d=PlaybackService.dj();if(d==null)return;if(d.auto)d.auto(false);else{List<MediaItem> songs=new ArrayList<>(mixQueue);if(songs.isEmpty())for(int i=0;i<player.getMediaItemCount();i++)songs.add(player.getMediaItemAt(i));if(songs.isEmpty()){message("Choose a playlist or song before turning on Auto Mix.");return;}MediaItem current=player.getCurrentMediaItem();long position=player.getCurrentPosition();boolean continuing=player.isPlaying()&&current!=null;
            d.pauseAll();d.queue(songs,mixRules);
            if(continuing){PlaybackService.claimDj();d.load(0,current,position);d.play(0);}
            d.auto(true);}message(d.message);refresh();}
    private void fadeTime(){String[] choices={"1 second","2 seconds","3 seconds","4 seconds","6 seconds","8 seconds"};int[] values={1,2,3,4,6,8};new AlertDialog.Builder(this).setTitle("Crossfade duration").setItems(choices,(d,n)->{getSharedPreferences("dj",0).edit().putInt("fade-seconds",values[n]).apply();DjEngine engine=PlaybackService.existingDj();if(engine!=null)engine.fadeSeconds(values[n]);message("Crossfade saved: "+choices[n]);}).setNegativeButton("Back",null).show();}
    private static String clock(long value){if(value<0||value==C.TIME_UNSET)return "--:--";long s=value/1000;return String.format(Locale.ROOT,"%d:%02d",s/60,s%60);}
    private void refresh(){if(trackTitle==null)return;androidx.media3.common.Player p=output();if(p==null)return;MediaItem item=p.getCurrentMediaItem();trackTitle.setText(item==null?"Choose music from Library or Playlists":item.mediaMetadata.title==null?item.mediaId:item.mediaMetadata.title);artist.setText(("dj".equals(PlaybackService.owner())?"Auto DJ · ":"TV Player · ")+(item==null||item.mediaMetadata.artist==null?"Playback on this TV":item.mediaMetadata.artist));play.setText(p.isPlaying()?"Pause":"Play");long duration=p.getDuration();time.setText(clock(p.getCurrentPosition())+" / "+clock(duration));seek.setEnabled(duration>0&&p.isCurrentMediaItemSeekable());if(!dragging&&duration>0)seek.setProgress((int)(p.getCurrentPosition()*1000/duration));DjEngine d=PlaybackService.existingDj();autoButton.setText("Auto Mix · "+(d!=null&&d.auto?"On":"Off"));}
    private void reloadLibrary(){message("Loading saved music…");worker.execute(()->{try{List<MediaItem> songs=LibraryStore.read(this);LinkedHashMap<String,MediaItem> all=new LinkedHashMap<>();for(MediaItem s:songs)all.put(s.mediaId,s);for(MediaItem s:RadioOffline.all(this))all.putIfAbsent(s.mediaId,s);runOnUiThread(()->{if(!active)return;library=new ArrayList<>(all.values());if(page.equals("Library"))show("Library",false);else message(library.size()+" saved songs · D-pad to browse");});}catch(Exception error){runOnUiThread(()->{if(active)message("Library could not load: "+error.getMessage()+". Retry from Library.");});}});}
    private void drawLibrary(){row(body,button("Add music folder",this::pickFolder),button("Refresh",this::reloadLibrary));EditText query=new EditText(this);query.setSingleLine(true);query.setHint("Search songs or artists");query.setTextColor(colors.text);query.setHintTextColor(colors.muted);query.setContentDescription("Search saved music");focusStyle(query);body.addView(query);
        ListView list=new ListView(this);list.setId(View.generateViewId());list.setItemsCanFocus(false);list.setFocusable(true);list.setSelector(tile(colors.surface,colors.accent));body.addView(list,new LinearLayout.LayoutParams(-1,0,1));
        List<MediaItem> snapshot=new ArrayList<>(library);displayed=new ArrayList<>(snapshot);ArrayAdapter<String> adapter=new ArrayAdapter<>(this,android.R.layout.simple_list_item_1,new ArrayList<>()){@Override public View getView(int position,View convert,ViewGroup parent){TextView row=(TextView)super.getView(position,convert,parent);row.setTextColor(colors.text);row.setTextSize(20);row.setMinHeight(dp(60));row.setGravity(Gravity.CENTER_VERTICAL);return row;}};list.setAdapter(adapter);
        int viewGeneration=generation;
        Runnable filter=()->{String value=query.getText().toString().toLowerCase(Locale.ROOT);worker.execute(()->{List<MediaItem> results=new ArrayList<>();List<String> labels=new ArrayList<>();for(MediaItem item:snapshot){String label=(item.mediaMetadata.title==null?item.mediaId:item.mediaMetadata.title.toString())+(item.mediaMetadata.artist==null?"":" — "+item.mediaMetadata.artist);if(label.toLowerCase(Locale.ROOT).contains(value)){results.add(item);labels.add(label);}}runOnUiThread(()->{if(!active||generation!=viewGeneration||!value.equals(query.getText().toString().toLowerCase(Locale.ROOT)))return;displayed=results;adapter.clear();adapter.addAll(labels);message(snapshot.isEmpty()?"No saved music yet. Add a folder, or sync playlists using Remote.":displayed.size()+" matching songs");});});};
        query.addTextChangedListener(new TextWatcher(){public void beforeTextChanged(CharSequence s,int a,int c,int f){}public void onTextChanged(CharSequence s,int a,int b,int c){handler.removeCallbacks(filter);handler.postDelayed(filter,180);}public void afterTextChanged(Editable e){}});filter.run();
        list.setOnItemClickListener((p,v,position,id)->{mixRules=new JSONObject();playSongs(new ArrayList<>(displayed),position);});
        if(snapshot.isEmpty())message("No saved music yet. Add a folder, or use Remote to save playlists offline.");
    }
    private void drawPlaylists(boolean offline){
        if(!offline)body.addView(button("Offline / synced playlists",()->show("Offline",true)));
        LinearLayout rows=new LinearLayout(this);rows.setOrientation(LinearLayout.VERTICAL);ScrollView scroll=new ScrollView(this);scroll.addView(rows);body.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
        message("Loading playlists…");int request=generation;
        worker.execute(()->{List<String> names=new ArrayList<>();List<JSONObject> rules=new ArrayList<>();List<File> folders=new ArrayList<>();String failure="";
            try{if(offline){for(File folder:RadioOffline.albums(this)){JSONObject album=RadioOffline.album(folder);names.add(album.optString("name",album.optString("station",folder.getName())));folders.add(folder);rules.add(null);}}
                else{JSONArray lists=PhonePlaylists.read(this);for(int i=0;i<lists.length();i++){JSONObject rule=lists.getJSONObject(i);names.add(rule.optString("name","Playlist")+(rule.optBoolean("smart")?" · Smart playlist":""));rules.add(rule);folders.add(null);}}}
            catch(Exception e){failure=e.getMessage();}String error=failure;
            runOnUiThread(()->{if(!active||request!=generation)return;message(error.isEmpty()?(names.isEmpty()?"No playlists saved on this TV yet. Use Remote to save a playlist offline.":"Choose a playlist to play; enable Auto Mix in Player if wanted."):"Could not read playlists: "+error);
                for(int i=0;i<names.size();i++){final int n=i;rows.addView(button(names.get(i),()->openPlaylist(rules.get(n),folders.get(n))));}});
        });
    }
    private void openPlaylist(JSONObject rule,File folder){message("Preparing selected playlist…");int request=generation;worker.execute(()->{try{List<MediaItem> songs=folder==null?PhonePlaylists.tracks(this,rule):RadioOffline.items(folder);runOnUiThread(()->{if(active&&request==generation){mixRules=rule==null?new JSONObject():rule;playSongs(songs,0);}});}catch(Exception e){runOnUiThread(()->{if(active&&request==generation)message("Playlist unavailable: "+e.getMessage());});}});}
    private void playSongs(List<MediaItem> songs,int at){if(player==null){message("Connecting to playback. Try again shortly.");return;}if(songs.isEmpty()){message("This playlist has no accessible songs. Check its saved sources or sync it from Remote.");return;}PlaybackService.switchToPlayer();mixQueue=new ArrayList<>(songs.subList(at,songs.size()));player.setMediaItems(songs,at,0);player.setRepeatMode(androidx.media3.common.Player.REPEAT_MODE_OFF);player.prepare();player.play();show("Player",true);message("Playing on this TV");}
    private void pickFolder(){try{startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION|Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION),80);}catch(ActivityNotFoundException e){message("This TV has no folder picker. Install a TV document picker or sync playlists using Remote.");}}
    @Override protected void onActivityResult(int request,int result,Intent data){
        super.onActivityResult(request,result,data);if(request!=80||result!=RESULT_OK||data==null||data.getData()==null)return;
        Uri tree=data.getData();try{getContentResolver().takePersistableUriPermission(tree,data.getFlags()&Intent.FLAG_GRANT_READ_URI_PERMISSION);}catch(SecurityException e){message("Cannot keep access to this folder. Choose a readable media source.");return;}
        message("Importing folder and subfolders…");
        worker.execute(()->{try{
            List<MediaItem> items=LibraryStore.read(this);Set<String> ids=new HashSet<>();for(MediaItem song:items)ids.add(song.mediaId);
            List<Uri> songs=FolderScanner.scan(getContentResolver(),tree);String warning="";try{FolderScanner.catalog(this,tree);}catch(Exception error){warning="Some saved preparation could not be read.";}
            SharedPreferences.Editor sources=getSharedPreferences("import-sources",0).edit(),folders=getSharedPreferences("phone-folders",0).edit();
            int skipped=0;
            for(Uri song:songs){if(Thread.currentThread().isInterrupted())return;try{
                String preparation=null;try{preparation=FolderScanner.preparation(getContentResolver(),tree,song);}catch(Exception error){warning="Some saved preparation could not be read.";}
                MediaItem item=LibraryStore.reference(this,song,song.getLastPathSegment(),preparation);
                if(ids.add(item.mediaId))items.add(item);
                sources.putString(song.toString(),item.mediaId);folders.putString(item.mediaId,android.provider.DocumentsContract.getTreeDocumentId(tree));
            }catch(Exception error){skipped++;}}
            LibraryStore.write(this,items);sources.apply();folders.apply();String report=items.size()+" indexed songs"+(skipped>0?" · "+skipped+" files skipped":"")+(warning.isEmpty()?"":" · "+warning);
            runOnUiThread(()->{if(active){reloadLibrary();message(report);}});
        }catch(Exception error){runOnUiThread(()->{if(active)message("Folder import failed: "+error.getMessage()+". Existing music was kept.");});}});
    }
    private void drawSettings(){LinearLayout screen=new LinearLayout(this);screen.setOrientation(LinearLayout.VERTICAL);scroll(screen);screen.addView(text("Full TV edition · Player, Library, Podcasts, DJ, Cast, Remote and Lab included. Editors remain available in More modules.",20));screen.addView(button("Appearance",()->new AlertDialog.Builder(this).setTitle("Appearance").setItems(new String[]{"Classic grey","Modern dark","Modern light"},(d,n)->{getSharedPreferences("settings",0).edit().putString("theme",new String[]{"classic","modern-dark","modern-light"}[n]).apply();colors=new Appearance(this);draw();show("Settings",false);}).setNegativeButton("Back",null).show()));screen.addView(button("Check for updates",()->{updating=true;AppUpdate.start(this);message(AppUpdate.status);}));screen.addView(button("Cancel update",()->{AppUpdate.cancel();message("Update cancelled; installed app kept.");}));screen.addView(button("GitHub releases",()->{try{startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse(AppUpdate.GITHUB+"/releases")));}catch(ActivityNotFoundException error){message("No browser installed. Open github.com/gitwide-cloud/amp-verse/releases on another device.");}}));screen.addView(button("More modules",()->new AlertDialog.Builder(this).setTitle("More modules").setItems(new String[]{"AMP Lab editor","Add-ons","Standard phone/tablet layout"},(d,n)->{if(n==0){if(Modules.embedded("lab"))startActivity(new Intent(this,cloud.gitwide.ampverse.lab.MainActivity.class));else Modules.open(this,"lab");}else if(n==1)Modules.show(this);else startActivity(new Intent(this,MainActivity.class).putExtra("phone-ui",true));}).setNegativeButton("Back",null).show()));}
    @Override public boolean dispatchKeyEvent(KeyEvent e){if(e.getAction()==KeyEvent.ACTION_DOWN&&e.getRepeatCount()==0){switch(e.getKeyCode()){case KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE:togglePlay();return true;case KeyEvent.KEYCODE_MEDIA_PLAY:androidx.media3.common.Player p=output();if(p!=null){p.prepare();p.play();}return true;case KeyEvent.KEYCODE_MEDIA_PAUSE:DjEngine paused=PlaybackService.existingDj();if("dj".equals(PlaybackService.owner())&&paused!=null)paused.pauseAll();else if(player!=null)player.pause();return true;case KeyEvent.KEYCODE_MEDIA_STOP:DjEngine stopped=PlaybackService.existingDj();if("dj".equals(PlaybackService.owner())&&stopped!=null)stopped.pauseAll();else if(player!=null)player.stop();return true;case KeyEvent.KEYCODE_MEDIA_NEXT:DjEngine d=PlaybackService.existingDj();if("dj".equals(PlaybackService.owner())&&d!=null)d.mixNext();else if(player!=null)player.seekToNextMediaItem();return true;case KeyEvent.KEYCODE_MEDIA_PREVIOUS:if(player!=null)player.seekToPreviousMediaItem();return true;default:break;}}return super.dispatchKeyEvent(e);}
    @Override public void onBackPressed(){if(page.equals("Cast")||page.equals("Remote")){show("Devices",false);rail.getChildAt(4).requestFocus();}else if(!page.equals("Player")){show("Player",false);rail.getChildAt(0).requestFocus();}else super.onBackPressed();}
}
