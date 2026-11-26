package cloud.gitwide.ampverse;
import java.util.concurrent.ConcurrentHashMap;
final class StationAudioStatus {
    private static final ConcurrentHashMap<String,String> VALUES=new ConcurrentHashMap<>();
    static String get(String server,String station){return VALUES.getOrDefault(server+"/"+station,"");}
    static void set(String server,String station,String text){if(VALUES.size()>32)VALUES.clear();VALUES.put(server+"/"+station,text);}
}
