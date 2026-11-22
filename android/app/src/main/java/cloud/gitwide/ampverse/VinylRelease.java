package cloud.gitwide.ampverse;

/** A later safety cancellation can revoke resume, but can never resurrect it. */
final class VinylRelease {
    private final boolean wasPlaying;private volatile boolean stopped,restore;
    VinylRelease(boolean wasPlaying){this.wasPlaying=wasPlaying;}
    synchronized boolean stop(boolean restoreState){boolean first=!stopped;if(first)restore=restoreState;else if(!restoreState)restore=false;stopped=true;return first;}
    boolean stopped(){return stopped;}
    boolean restore(){return restore;}
    boolean play(){return restore&&wasPlaying;}
}
