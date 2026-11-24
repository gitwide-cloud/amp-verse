package cloud.gitwide.ampverse;

import java.io.IOException;
import java.util.Arrays;
import java.util.concurrent.atomic.AtomicBoolean;

/** Ordered, identity-bound ownership of one selected server's live ingestion. */
final class PhoneLiveSession {
    static final int MAX_PACKET = 8192;
    enum State { IDLE, STARTING, LIVE, RECONNECTING, STOPPED, FAILED }
    static final class Identity {
        final String server, station, library;
        Identity(String server, String station, String library) {
            this.server = identifier(server); this.station = identifier(station); this.library = identifier(library);
        }
        @Override public boolean equals(Object other) {
            if (!(other instanceof Identity)) return false;
            Identity value=(Identity)other;
            return server.equals(value.server)&&station.equals(value.station)&&library.equals(value.library);
        }
        @Override public int hashCode() { return (server.hashCode()*31+station.hashCode())*31+library.hashCode(); }
        private static String identifier(String value) {
            if(value==null||value.isEmpty()||value.length()>512||value.chars().anyMatch(c->c<33||c>126))throw new IllegalArgumentException("Host identity required");
            return value;
        }
    }
    static final class Started {
        final String token;
        final long nextSequence;
        final boolean sequenced, guardedResume;
        Started(String token,long nextSequence,boolean sequenced,boolean guardedResume) {
            if(token==null||token.isEmpty()||token.length()>1024||token.chars().anyMatch(c->c<33||c>126))throw new IllegalArgumentException("Invalid live session");
            if(nextSequence<0||nextSequence==Long.MAX_VALUE)throw new IllegalArgumentException("Invalid upload sequence");
            this.token=token;this.nextSequence=nextSequence;this.sequenced=sequenced;this.guardedResume=guardedResume;
        }
    }
    static class Rejected extends IOException { Rejected(String message){super(message);} }
    interface Transport {
        Identity verify() throws IOException;
        Started start(String previousToken) throws IOException;
        long upload(String token,long sequence,byte[] mp3) throws IOException;
        void stop(String token) throws IOException;
        /** Must promptly abort only this transport's in-flight request. */
        void abort();
    }

    private final Identity expected;
    private final Transport transport;
    private final AtomicBoolean cancelled=new AtomicBoolean();
    private volatile State state=State.IDLE;
    private String token;
    private long sequence;
    private boolean stopAcknowledged;

    PhoneLiveSession(Identity expected,Transport transport) {
        if(expected==null||transport==null)throw new IllegalArgumentException("Selected host required");
        this.expected=expected;this.transport=transport;
    }
    State state(){return state;}
    synchronized long nextSequence(){return sequence;}
    synchronized boolean stopAcknowledged(){return stopAcknowledged;}

    synchronized void begin() throws IOException {
        if(state!=State.IDLE)throw new Rejected("This sender already has an owner");
        checkCancelled();state=State.STARTING;
        try { verify();accept(transport.start(null)); }
        catch (IOException|RuntimeException error) {state=State.FAILED;throw error;}
    }

    private void verify() throws IOException {
        checkCancelled();
        if(!expected.equals(transport.verify()))throw new Rejected("Selected host identity changed; select it again");
        checkCancelled();
    }

    private void accept(Started started) throws IOException {
        token=started.token;sequence=started.nextSequence;
        if(!started.sequenced||!started.guardedResume){
            stopOwned();throw new Rejected("Update the selected host for reliable phone broadcasting");
        }
        try {verify();}
        catch(IOException error){stopOwned();throw error;}
        state=State.LIVE;
    }

    /** Retrying a lost acknowledgement uses the same bytes, token and sequence. */
    synchronized void send(byte[] encoded,int length) throws IOException {
        checkCancelled();if(state!=State.LIVE)throw new Rejected("Live sender is not ready");
        if(encoded==null||length<1||length>MAX_PACKET||length>encoded.length)throw new IllegalArgumentException("Invalid encoded packet");
        if(sequence==Long.MAX_VALUE)throw new Rejected("Live upload sequence exhausted");
        byte[] packet=Arrays.copyOf(encoded,length);
        for(int attempt=0;attempt<2;attempt++) {
            checkCancelled();
            try {
                long acknowledged=transport.upload(token,sequence,packet);
                checkCancelled();
                if(acknowledged!=sequence+1)throw new Rejected("Live upload acknowledgement changed");
                sequence=acknowledged;return;
            } catch(Rejected error) {state=State.FAILED;throw error;}
            catch(IOException error) {
                if(cancelled.get())throw new Rejected("Phone broadcast cancelled");
                if(attempt==1){state=State.RECONNECTING;throw error;}
            }
        }
    }

    /** Caller restarts its encoder from current master PCM after a guarded resume. */
    synchronized void resume() throws IOException {
        checkCancelled();if(state!=State.RECONNECTING||token==null)throw new Rejected("No live owner can be resumed");
        try {verify();accept(transport.start(token));}
        catch(IOException|RuntimeException error){state=State.FAILED;throw error;}
    }

    /** Safe on the UI thread: does not wait for an encoder or a network timeout. */
    void cancel(){cancelled.set(true);try{transport.abort();}catch(RuntimeException ignored){}}

    /** Called by the sender worker, after cancel has interrupted any active I/O. */
    synchronized void finish() {
        cancelled.set(true);stopOwned();state=State.STOPPED;
    }

    private void stopOwned() {
        if(token==null)return;
        try {transport.stop(token);stopAcknowledged=true;}
        catch(IOException ignored){stopAcknowledged=false;}
        token=null;
    }
    private void checkCancelled() throws Rejected {if(cancelled.get())throw new Rejected("Phone broadcast cancelled");}
}
