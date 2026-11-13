package cloud.gitwide.ampverse;

import androidx.media3.common.MediaItem;
import java.util.*;
import org.json.JSONObject;

/** Lightweight decision policy over desktop-prepared data; no analysis or audio ownership. */
final class DjLogic {
    static long cueIn(MediaItem item){double value=DjEngine.preparation(item).optDouble("cue_in",0);return Double.isFinite(value)?Math.max(0,(long)(value*1000)):0;}
    static long cueOut(MediaItem item,long duration){double seconds=DjEngine.preparation(item).optDouble("cue_out",-1);long end=Double.isFinite(seconds)?(long)(seconds*1000):-1;return end>cueIn(item)?(duration>0?Math.min(duration,end):end):duration;}
    static long entry(MediaItem item,JSONObject rules){return PhoneSmartRules.entry(item,rules);}
    static List<MediaItem> repeatCandidates(List<MediaItem> source,Collection<String> recent,String current,int avoid){return PhoneSmartRules.repeatCandidates(source,recent,current,avoid);}
    static float tempoRatio(MediaItem outgoing,MediaItem incoming,float outgoingSpeed){
        double a=DjEngine.preparation(outgoing).optDouble("bpm",0),b=DjEngine.preparation(incoming).optDouble("bpm",0);
        if(!Double.isFinite(a)||!Double.isFinite(b)||a<=0||b<=0)return 1;
        double ratio=a*outgoingSpeed/b;if(ratio<.65)ratio*=2;else if(ratio>1.5)ratio/=2;
        return ratio>=.92&&ratio<=1.08?(float)ratio:1;
    }
}
