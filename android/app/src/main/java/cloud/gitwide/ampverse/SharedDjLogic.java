package cloud.gitwide.ampverse;
/** JNI adapter only. Algorithms live in crates/amp-dj-policy for every platform. */
final class SharedDjLogic {
    static {System.loadLibrary("amp_dj_policy");if(version()!=1)throw new IllegalStateException("Unsupported DJ Logic version");}
    static native int version();
    static native double tempoRate(double outgoing,double rate,double incoming,double limit);
    static native double distance(double outgoing,double incoming);
    static native double boundary(double anchor,double period,double position);
    static native double fade(double requested,double remaining);
    static native double pairCost(double a,double b,double ae,double be,double target,int ak,int bk);
    static int keyCode(String key){if(key==null)return 0;key=key.trim().toUpperCase(java.util.Locale.ROOT);if(!key.matches("(1[0-2]|[1-9])[AB]"))return 0;return Integer.parseInt(key.substring(0,key.length()-1))+(key.endsWith("B")?12:0);}
}
