package cloud.gitwide.ampverse;
/** Recovery never acquires playback ownership or overrides a user/focus pause. */
final class LiveRecoveryPolicy {
    static boolean allowed(boolean station,boolean requested,boolean focusSuppressed,boolean djOwner){return station&&requested&&!focusSuppressed&&!djOwner;}
    static boolean retryError(boolean allowed,boolean error,boolean ended){return allowed&&(error||ended);}
    static boolean preparationExpired(long since,long now){return since>0&&now-since>=10000;}
    static boolean stalled(long since,long now){return since>0&&now-since>=6000;}
    static boolean behind(long offset,long now,long lastCatchup){return offset>5000&&now-lastCatchup>10000;}
}
