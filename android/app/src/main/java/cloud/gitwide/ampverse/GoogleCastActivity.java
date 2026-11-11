package cloud.gitwide.ampverse;
import android.os.Bundle;
import android.widget.*;
import androidx.appcompat.app.AppCompatActivity;
import androidx.mediarouter.app.MediaRouteButton;
import com.google.android.gms.cast.*;
import com.google.android.gms.cast.framework.*;
/** Native Google Cast picker; the speaker reads the station stream directly. */
public final class GoogleCastActivity extends AppCompatActivity {
    private TextView status;
    private final SessionManagerListener<CastSession> sessions=new SessionManagerListener<CastSession>() {
        public void onSessionStarting(CastSession s){status.setText("Connecting to speaker...");}
        public void onSessionStarted(CastSession s,String id){play();}
        public void onSessionStartFailed(CastSession s,int code){status.setText("Speaker connection failed: "+code);}
        public void onSessionEnding(CastSession s){}
        public void onSessionEnded(CastSession s,int code){status.setText("Casting stopped");}
        public void onSessionResuming(CastSession s,String id){status.setText("Reconnecting to speaker...");}
        public void onSessionResumed(CastSession s,boolean suspended){status.setText("Speaker reconnected. Tap Play to resume this station.");}
        public void onSessionResumeFailed(CastSession s,int code){status.setText("Speaker reconnect failed: "+code);}
        public void onSessionSuspended(CastSession s,int reason){status.setText("Speaker disconnected; phone playback is unchanged.");}
    };
    @Override protected void onResume(){super.onResume();try{CastContext.getSharedInstance(this).getSessionManager().addSessionManagerListener(sessions,CastSession.class);}catch(Exception ignored){}}
    @Override protected void onPause(){try{CastContext.getSharedInstance(this).getSessionManager().removeSessionManagerListener(sessions,CastSession.class);}catch(Exception ignored){}super.onPause();}

    @Override public void onCreate(Bundle state){
        super.onCreate(state);
        LinearLayout content=new LinearLayout(this);content.setOrientation(LinearLayout.VERTICAL);int pad=(int)(20*getResources().getDisplayMetrics().density);content.setPadding(pad,pad,pad,pad);setContentView(content);
        TextView heading=new TextView(this);heading.setText("Google Home / Chromecast");heading.setTextSize(24);content.addView(heading);
        status=new TextView(this);status.setText("Choose a speaker with the Cast button to play this station or live DJ broadcast.");status.setTextSize(17);content.addView(status);
        try {
            CastContext.getSharedInstance(this);
            MediaRouteButton route=new MediaRouteButton(this);route.setContentDescription("Choose Google Cast speaker");content.addView(route,new LinearLayout.LayoutParams(pad*3,pad*3));CastButtonFactory.setUpMediaRouteButton(this,route);
            Button play=new Button(this);play.setText("Play station on speaker");content.addView(play);play.setOnClickListener(v->play());
            Button stop=new Button(this);stop.setText("Stop casting");content.addView(stop);stop.setOnClickListener(v->{CastContext.getSharedInstance(this).getSessionManager().endCurrentSession(true);status.setText("Casting stopped");});
        } catch(Exception error){status.setText("Google Cast is unavailable: "+error.getMessage());}
        Button back=new Button(this);back.setText("Back to AMP Cast");content.addView(back);back.setOnClickListener(v->finish());
    }
    private void play(){
        try {
            String url=getIntent().getStringExtra("stream");android.net.Uri uri=android.net.Uri.parse(url==null?"":url);String host=uri.getHost();
            if(host==null||host.equals("localhost")||host.equals("127.0.0.1")||!"http".equals(uri.getScheme())){status.setText("Connect to your Cast server using its LAN address first. A speaker cannot use the phone's localhost address.");return;}
            CastSession session=CastContext.getSharedInstance(this).getSessionManager().getCurrentCastSession();
            if(session==null||!session.isConnected()||session.getRemoteMediaClient()==null){status.setText("Choose a Google speaker first, then tap Play station on speaker.");return;}
            MediaMetadata metadata=new MediaMetadata(MediaMetadata.MEDIA_TYPE_MUSIC_TRACK);metadata.putString(MediaMetadata.KEY_TITLE,getIntent().getStringExtra("title"));
            MediaInfo media=new MediaInfo.Builder(url).setContentType("audio/mpeg").setStreamType(MediaInfo.STREAM_TYPE_LIVE).setMetadata(metadata).build();
            session.getRemoteMediaClient().load(new MediaLoadRequestData.Builder().setMediaInfo(media).setAutoplay(true).build()).setResultCallback(result->status.setText(result.getStatus().isSuccess()?"Stream sent to speaker. Use the Cast button for playback controls.":"Speaker rejected the stream: "+result.getStatus().getStatusCode()));
        }catch(Exception error){status.setText("Could not cast: "+error.getMessage());}
    }
}
