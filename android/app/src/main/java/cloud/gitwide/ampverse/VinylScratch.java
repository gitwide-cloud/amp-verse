package cloud.gitwide.ampverse;
import android.content.Context;
import android.media.*;
import android.os.*;
import androidx.media3.common.*;
import androidx.media3.exoplayer.ExoPlayer;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

/** Four bounded local windows; only a held deck owns a scratch AudioTrack. */
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
final class VinylScratch {
    interface Barrier {void quiet(Runnable afterPause); }
    private static final class Slot {
        long epoch,retryAfter;String key="";VinylPcm window;Request pending,running;Session session;
    }
    private static final class Request {
        final int deck;final long epoch,position;final String key;final android.net.Uri uri;final AtomicBoolean cancelled=new AtomicBoolean();
        Request(int deck,long epoch,String key,android.net.Uri uri,long position){this.deck=deck;this.epoch=epoch;this.key=key;this.uri=uri;this.position=position;}
    }
    private static final class Session {
        final int deck;final long epoch,token;final String key;final VinylPcm pcm;
        final VinylRelease release;volatile float gain;volatile AudioTrack track;long playedFrames;boolean launched;
        Session(int deck,long epoch,long token,String key,ExoPlayer player,VinylPcm window){this.deck=deck;this.epoch=epoch;this.token=token;this.key=key;release=new VinylRelease(player.getPlayWhenReady());gain=player.getVolume();pcm=new VinylPcm(window.samples,window.rate,window.startMs,player.getCurrentPosition());pcm.hold(System.nanoTime());}
        void stop(boolean resume){synchronized(this){if(!release.stop(resume))return;AudioTrack current=track;if(current!=null)try{current.pause();playedFrames=current.getPlaybackHeadPosition()&0xffffffffL;current.flush();}catch(IllegalStateException ignored){}}}
    }
    private final Context context;private final ExoPlayer[] decks;private final Consumer<String> message;private final Barrier owner;
    private final Handler main=new Handler(Looper.getMainLooper());private final Slot[] slots={new Slot(),new Slot(),new Slot(),new Slot()};
    private final ExecutorService preparation=Executors.newSingleThreadExecutor(r->{Thread t=new Thread(r,"amp-vinyl-prepare");t.setDaemon(true);return t;});
    private final Object gate=new Object();private boolean pump,closed;private long nextToken;private VinylDecoder decoder;
    VinylScratch(Context context,ExoPlayer[] decks,Consumer<String> message){this(context,decks,message,Runnable::run);}
    VinylScratch(Context context,ExoPlayer[] decks,Consumer<String> message,Barrier owner){this.context=context.getApplicationContext();this.decks=decks;this.message=message;this.owner=owner;}
    private String key(int deck){MediaItem item=decks[deck].getCurrentMediaItem();return item==null||item.localConfiguration==null?"":item.mediaId+"\n"+item.localConfiguration.uri;}
    boolean busy(int deck){return slots[deck].session!=null;}
    boolean any(){for(int i=0;i<4;i++)if(busy(i))return true;return false;}
    void gain(int deck,float value){Session s=slots[deck].session;if(s!=null)s.gain=Math.max(0,Math.min(1,value));}
    boolean ready(int deck){Slot slot=slots[deck];return !closed&&slot.window!=null&&slot.key.equals(key(deck))&&slot.window.covers(decks[deck].getCurrentPosition(),0);}
    void prepare(int deck){
        if(closed||busy(deck)||!VinylDecoder.available())return;ExoPlayer player=decks[deck];MediaItem item=player.getCurrentMediaItem();
        if(item==null||item.localConfiguration==null)return;String key=key(deck),scheme=item.localConfiguration.uri.getScheme();
        if(!"file".equals(scheme)&&!"content".equals(scheme))return;
        long position=player.getCurrentPosition();Slot slot=slots[deck];if(android.os.SystemClock.elapsedRealtime()<slot.retryAfter)return;
        synchronized(gate){
            if(slot.window!=null&&slot.key.equals(key)&&slot.window.covers(position,1000))return;
            Request prior=slot.pending!=null?slot.pending:slot.running;
            if(prior!=null&&prior.key.equals(key)&&Math.abs(prior.position-position)<1500)return;
            if(prior!=null)prior.cancelled.set(true);
            slot.epoch++;slot.pending=new Request(deck,slot.epoch,key,item.localConfiguration.uri,position);
            if(!pump){pump=true;preparation.execute(this::prepareLoop);}
        }
    }
    private void prepareLoop(){
        while(true){Request request=null;synchronized(gate){for(Slot slot:slots)if(slot.pending!=null){request=slot.pending;slot.pending=null;slot.running=request;break;}if(request==null){pump=false;return;}}
            Request work=request;VinylPcm result=null;String error=null;
            try{if(decoder==null)decoder=VinylDecoder.full();if(decoder==null)throw new IllegalStateException("Vinyl scratch needs the Full DJ decoder");result=decoder.decode(context,work.uri,work.position,work.cancelled);}catch(Exception failure){error="Scratch preparation unavailable. Use a local/offline song, then reload the deck and retry.";}
            VinylPcm prepared=result;String problem=error;
            synchronized(gate){if(slots[work.deck].running==work)slots[work.deck].running=null;}
            main.post(()->{Slot slot=slots[work.deck];if(closed||work.cancelled.get()||slot.epoch!=work.epoch||!key(work.deck).equals(work.key))return;if(prepared!=null){slot.key=work.key;slot.window=prepared;}else {slot.retryAfter=android.os.SystemClock.elapsedRealtime()+10000;message.accept(problem);}});
        }
    }
    boolean begin(int deck){
        if(closed||busy(deck))return false;
        if(!VinylDecoder.available()){message.accept("Real vinyl scratch requires AMP Verse Full with the local-audio decoder. Open or update Full DJ; no seek imitation was used.");return false;}
        MediaItem source=decks[deck].getCurrentMediaItem();
        if(source==null||source.localConfiguration==null||!("file".equals(source.localConfiguration.uri.getScheme())||"content".equals(source.localConfiguration.uri.getScheme()))){message.accept("Scratch needs a local or downloaded song. Current playback was not changed.");return false;}
        if(!ready(deck)){prepare(deck);message.accept("Vinyl is preparing local audio. Playback was not interrupted; wait, then touch the record again.");return false;}
        Slot slot=slots[deck];Session session=new Session(deck,slot.epoch,++nextToken,key(deck),decks[deck],slot.window);slot.session=session;
        decks[deck].pause();message.accept("Vinyl held · rotate forward or backward to scratch.");
        // This message is ordered after pause on ExoPlayer's playback thread.
        // Scratch cannot start until that thread has stopped its normal renderer.
        decks[deck].createMessage((type,payload)->main.post(()->handoff(session))).send();
        main.postDelayed(()->{if(slots[deck].session==session&&!session.launched){session.stop(false);finish(session,"Scratch handoff timed out; press Play to resume.");}},1500);
        return true;
    }
    private void handoff(Session session){
        Slot slot=slots[session.deck];
        if(closed||slot.session!=session||session.release.stopped()||slot.epoch!=session.epoch||!key(session.deck).equals(session.key)){finish(session,null);return;}
        owner.quiet(()->activate(session));
    }
    private void activate(Session session){
        Slot slot=slots[session.deck];if(closed||slot.session!=session||slot.epoch!=session.epoch||!key(session.deck).equals(session.key)){session.stop(false);finish(session,null);return;}
        if(session.release.stopped()){finish(session,null);return;}session.launched=true;
        Thread voice=new Thread(()->voice(session),"amp-vinyl-voice-"+session.deck);voice.setDaemon(true);voice.start();
    }
    long token(int deck){Session session=slots[deck].session;return session==null?0:session.token;}
    void drag(int deck,long token,double seconds,long now){Session session=slots[deck].session;if(session!=null&&session.token==token&&!session.release.stopped())session.pcm.drag(seconds*session.pcm.rate,now);}
    void end(int deck,boolean resume){Session session=slots[deck].session;if(session!=null){session.stop(resume);if(!session.launched)finish(session,null);}}
    void end(int deck,long token,boolean resume){Session session=slots[deck].session;if(session!=null&&session.token==token)end(deck,resume);}
    void invalidate(int deck){Slot slot=slots[deck];synchronized(gate){slot.epoch++;slot.retryAfter=0;slot.window=null;slot.key="";if(slot.pending!=null)slot.pending.cancelled.set(true);slot.pending=null;if(slot.running!=null)slot.running.cancelled.set(true);}end(deck,false);}
    void releaseAll(){for(int i=0;i<4;i++)end(i,true);}
    void pauseAll(){for(int i=0;i<4;i++)end(i,false);}
    void cancelAll(){for(int i=0;i<4;i++)invalidate(i);}
    void close(){closed=true;cancelAll();preparation.shutdownNow();}
    private void voice(Session session){
        String error=null;AudioTrack track=null;
        try{
            android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_AUDIO);
            int minimum=AudioTrack.getMinBufferSize(48000,AudioFormat.CHANNEL_OUT_STEREO,AudioFormat.ENCODING_PCM_16BIT);
            if(minimum<=0||minimum>48000)throw new IllegalStateException("Low-latency scratch output unavailable");
            track=new AudioTrack.Builder().setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_MEDIA).setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC).build()).setAudioFormat(new AudioFormat.Builder().setSampleRate(48000).setEncoding(AudioFormat.ENCODING_PCM_16BIT).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build()).setTransferMode(AudioTrack.MODE_STREAM).setBufferSizeInBytes(Math.max(minimum,1920)).setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY).build();
            if(track.getState()!=AudioTrack.STATE_INITIALIZED)throw new IllegalStateException("Scratch output not initialized");
            synchronized(session){session.track=track;if(!session.release.stopped())track.play();}
            short[] pcm=new short[480];
            AudioDsp dsp=new AudioDsp(false);dsp.configure(new androidx.media3.common.audio.AudioProcessor.AudioFormat(48000,2,C.ENCODING_PCM_16BIT));dsp.flush();
            java.nio.ByteBuffer input=java.nio.ByteBuffer.allocateDirect(960).order(java.nio.ByteOrder.LITTLE_ENDIAN);
            while(!session.release.stopped()){
                session.pcm.render(pcm,240);
                input.clear();for(short sample:pcm)input.putShort(sample);input.flip();dsp.queueInput(input);java.nio.ByteBuffer processed=dsp.getOutput().order(java.nio.ByteOrder.LITTLE_ENDIAN);for(int i=0;i<pcm.length;i++)pcm[i]=processed.getShort();
                int offset=0;
                while(offset<pcm.length&&!session.release.stopped()){
                    track.setVolume(session.gain);int wrote=track.write(pcm,offset,pcm.length-offset,AudioTrack.WRITE_NON_BLOCKING);
                    if(wrote<0)throw new IllegalStateException("Scratch output stopped");if(wrote==0)Thread.sleep(2);else offset+=wrote;
                }
            }
        }catch(Exception failure){if(!session.release.stopped()){session.stop(false);error="Scratch output unavailable. Normal deck is paused; press Play to retry.";}}
        finally{if(track!=null){synchronized(session){if(!session.release.stopped())session.stop(false);try{track.pause();track.flush();}catch(IllegalStateException ignored){}session.track=null;}try{track.release();}catch(IllegalStateException ignored){}}String problem=error;main.post(()->finish(session,problem));}
    }
    private void finish(Session session,String error){
        Slot slot=slots[session.deck];if(slot.session!=session)return;slot.session=null;
        if(closed||slot.epoch!=session.epoch||!key(session.deck).equals(session.key))return;
        ExoPlayer player=decks[session.deck];long position=session.pcm.heardPositionMs(session.playedFrames);
        player.seekTo(position); // Scratch never changes the deck tempo; preserve explicit slider/Sync edits.
        if(session.release.restore())player.setPlayWhenReady(session.release.play());
        if(error!=null)message.accept(error);else message.accept(session.release.restore()?"Vinyl released · deck restored at the scratch position.":"Scratch cancelled · deck paused. Press Play when ready.");
    }
    long position(int deck){Session session=slots[deck].session;return session==null?decks[deck].getCurrentPosition():session.pcm.positionMs();}
}
