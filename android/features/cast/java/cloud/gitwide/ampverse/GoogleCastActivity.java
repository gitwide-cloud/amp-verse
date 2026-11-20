package cloud.gitwide.ampverse;
import android.os.Bundle;
import android.widget.*;
import androidx.appcompat.app.AppCompatActivity;
import androidx.mediarouter.app.MediaRouteButton;
import com.google.android.gms.cast.*;
import com.google.android.gms.cast.framework.*;
import com.google.android.gms.cast.framework.media.RemoteMediaClient;
/** Native Google Cast picker; the speaker reads the station stream directly. */
public final class GoogleCastActivity extends AppCompatActivity {
    private TextView status;
    private Appearance colors;
    private RemoteMediaClient observed;
    private final RemoteMediaClient.Callback playback=new RemoteMediaClient.Callback(){
        @Override public void onStatusUpdated(){showPlayback();}
    };
    private void observe(CastSession session){
        RemoteMediaClient next=session==null?null:session.getRemoteMediaClient();
        if(observed==next)return;
        if(observed!=null)observed.unregisterCallback(playback);
        observed=next;if(observed!=null)observed.registerCallback(playback);
    }
    private void showPlayback(){
        if(observed==null)return;
        MediaInfo media=observed.getMediaInfo();
        String requested=getIntent().getStringExtra("stream");
        if(media!=null&&!java.util.Objects.equals(requested,media.getContentId())){
            status.setText("Speaker is playing another source. Tap Play to replace it with this host stream.");return;
        }
        switch(observed.getPlayerState()){
            case MediaStatus.PLAYER_STATE_PLAYING:status.setText("Receiver reports playing the host stream. Physical audibility is unverified.");break;
            case MediaStatus.PLAYER_STATE_BUFFERING:status.setText("Speaker is buffering the host stream…");break;
            case MediaStatus.PLAYER_STATE_PAUSED:status.setText("Speaker playback is paused.");break;
            case MediaStatus.PLAYER_STATE_IDLE:status.setText(observed.getIdleReason()==MediaStatus.IDLE_REASON_ERROR?"Speaker could not play the stream. Check that it can reach the host LAN address and the station is on air.":"Speaker is idle. Tap Play to send this host stream.");break;
            default:status.setText("Waiting for receiver playback status.");
        }
    }
    private final SessionManagerListener<CastSession> sessions=new SessionManagerListener<CastSession>() {
        public void onSessionStarting(CastSession s){status.setText("Connecting to speaker...");}
        public void onSessionStarted(CastSession s,String id){play();}
        public void onSessionStartFailed(CastSession s,int code){status.setText("Speaker connection failed: "+code);}
        public void onSessionEnding(CastSession s){}
        public void onSessionEnded(CastSession s,int code){observe(null);status.setText("Casting stopped");}
        public void onSessionResuming(CastSession s,String id){status.setText("Reconnecting to speaker...");}
        public void onSessionResumed(CastSession s,boolean suspended){observe(s);showPlayback();}
        public void onSessionResumeFailed(CastSession s,int code){status.setText("Speaker reconnect failed: "+code);}
        public void onSessionSuspended(CastSession s,int reason){status.setText("Speaker disconnected; phone playback is unchanged.");}
    };
    @Override protected void onResume(){super.onResume();try{CastContext.getSharedInstance(this).getSessionManager().addSessionManagerListener(sessions,CastSession.class);observe(CastContext.getSharedInstance(this).getSessionManager().getCurrentCastSession());showPlayback();}catch(Exception ignored){}}
    @Override protected void onPause(){try{CastContext.getSharedInstance(this).getSessionManager().removeSessionManagerListener(sessions,CastSession.class);}catch(Exception ignored){}observe(null);super.onPause();}

    @Override public void onCreate(Bundle state){
        super.onCreate(state);
        colors=new Appearance(this);getWindow().setStatusBarColor(colors.background);getWindow().setNavigationBarColor(colors.background);
        ScrollView scroll=new ScrollView(this);scroll.setBackgroundColor(colors.background);scroll.setOnApplyWindowInsetsListener((view,insets)->{view.setPadding(insets.getSystemWindowInsetLeft(),insets.getSystemWindowInsetTop(),insets.getSystemWindowInsetRight(),insets.getSystemWindowInsetBottom());return insets;});
        LinearLayout content=new LinearLayout(this);content.setOrientation(LinearLayout.VERTICAL);int pad=(int)(20*getResources().getDisplayMetrics().density);content.setPadding(pad,pad,pad,pad);scroll.addView(content);setContentView(scroll);
        TextView heading=new TextView(this);heading.setText("Google Home / Chromecast");heading.setTextSize(24);heading.setTextColor(colors.text);content.addView(heading);
        status=new TextView(this);status.setText("Choose a Google speaker. It fetches the host stream directly; phone audio is not sent.");status.setTextSize(17);status.setTextColor(colors.muted);content.addView(status);
        TextView source=new TextView(this);source.setTextColor(colors.text);String title=getIntent().getStringExtra("title");source.setText("Host station: "+(title==null||title.trim().isEmpty()?"Selected station":title));content.addView(source);
        try {
            CastContext.getSharedInstance(this);
            boolean light=getSharedPreferences("settings",MODE_PRIVATE).getString("theme","classic").equals("modern-light");
            android.content.Context pickerContext=new android.view.ContextThemeWrapper(this,light?androidx.appcompat.R.style.Theme_AppCompat_Light:androidx.appcompat.R.style.Theme_AppCompat);
            MediaRouteButton route=new MediaRouteButton(pickerContext);route.setContentDescription("Choose Google Cast speaker");content.addView(route,new LinearLayout.LayoutParams(pad*3,pad*3));CastButtonFactory.setUpMediaRouteButton(this,route);
            Button play=button();play.setText("Play station on speaker");content.addView(play);play.setOnClickListener(v->play());
            Button stop=button();stop.setText("Stop casting");content.addView(stop);stop.setOnClickListener(v->{SessionManager manager=CastContext.getSharedInstance(this).getSessionManager();if(manager.getCurrentCastSession()==null){status.setText("No Google Cast session is active.");return;}manager.endCurrentSession(true);status.setText("Stopping Google Cast session…");});
        } catch(Exception error){status.setText("Google Cast is unavailable: "+error.getMessage());}
        Button back=button();back.setText("Back to AMP Cast");content.addView(back);back.setOnClickListener(v->finish());
    }
    private Button button(){Button button=new Button(this);button.setAllCaps(false);button.setMinHeight((int)(48*getResources().getDisplayMetrics().density));button.setTextColor(colors.text);button.setBackground(Chrome.button(colors,false));return button;}
    private void play(){
        try {
            String url=getIntent().getStringExtra("stream");android.net.Uri uri=android.net.Uri.parse(url==null?"":url);String host=uri.getHost();
            if(host==null||host.equalsIgnoreCase("localhost")||host.startsWith("127.")||host.equals("0.0.0.0")||host.equals("::")||host.equals("[::]")||host.equals("::1")||host.equals("[::1]")||!("http".equals(uri.getScheme())||"https".equals(uri.getScheme()))){status.setText("Connect to your Cast server using its LAN address first. A speaker cannot use the phone's localhost address.");return;}
            CastSession session=CastContext.getSharedInstance(this).getSessionManager().getCurrentCastSession();
            if(session==null||!session.isConnected()||session.getRemoteMediaClient()==null){status.setText("Choose a Google speaker first, then tap Play station on speaker.");return;}
            observe(session);
            MediaMetadata metadata=new MediaMetadata(MediaMetadata.MEDIA_TYPE_MUSIC_TRACK);metadata.putString(MediaMetadata.KEY_TITLE,getIntent().getStringExtra("title"));
            MediaInfo media=new MediaInfo.Builder(url).setContentType("audio/mpeg").setStreamType(MediaInfo.STREAM_TYPE_LIVE).setMetadata(metadata).build();
            session.getRemoteMediaClient().load(new MediaLoadRequestData.Builder().setMediaInfo(media).setAutoplay(true).build()).setResultCallback(result->{if(isFinishing()||isDestroyed())return;if(result.getStatus().isSuccess()){status.setText("Load accepted; waiting for receiver playback status.");if(observed!=null){observed.requestStatus();}}else status.setText("Speaker rejected the stream: "+result.getStatus().getStatusCode());});
        }catch(Exception error){status.setText("Could not cast: "+error.getMessage());}
    }
}
