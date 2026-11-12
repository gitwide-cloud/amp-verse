package cloud.gitwide.ampverse;

import android.content.*;
import androidx.media3.common.MediaItem;
import java.util.*;
import org.json.*;

/** Phone drafts and overrides stay local; reconnection never publishes them to a NAS. */
final class PhonePlaylists {
    static JSONArray read(Context context){try{return new JSONArray(context.getSharedPreferences("phone-playlists",0).getString("lists","[{\"id\":\"demo\",\"name\":\"Demo\",\"smart\":false,\"songs\":[]}]"));}catch(JSONException error){throw new IllegalStateException("Saved playlists could not be read",error);}}
    static void save(Context context,JSONObject list)throws JSONException {JSONArray all=read(context),next=new JSONArray();boolean found=false;for(int i=0;i<all.length();i++){JSONObject old=all.getJSONObject(i);if(old.getString("id").equals(list.getString("id"))){next.put(list);found=true;}else next.put(old);}if(!found)next.put(list);if(!context.getSharedPreferences("phone-playlists",0).edit().putString("lists",next.toString()).commit())throw new IllegalStateException("Playlist could not be saved");}
    static void remove(Context context,String id)throws JSONException{JSONArray next=new JSONArray(),all=read(context);for(int i=0;i<all.length();i++)if(!all.getJSONObject(i).getString("id").equals(id))next.put(all.getJSONObject(i));if(!context.getSharedPreferences("phone-playlists",0).edit().putString("lists",next.toString()).commit())throw new IllegalStateException("Playlist could not be removed");}
    static double bpm(Context context,MediaItem item){return context.getSharedPreferences("phone-track-details",0).getFloat(item.mediaId+":bpm",(float)DjEngine.preparation(item).optDouble("bpm",0));}
    static double energy(Context context,MediaItem item){double e=knownEnergy(context,item);return Double.isFinite(e)?Math.max(0,Math.min(1,e)):.5;}
    static double knownEnergy(Context context,MediaItem item){android.content.SharedPreferences prefs=context.getSharedPreferences("phone-track-details",0);if(prefs.contains(item.mediaId+":energy"))return prefs.getFloat(item.mediaId+":energy",0);JSONObject p=DjEngine.preparation(item);return PhoneSmartRules.extensions(p).optDouble("energy",Double.NaN);}
    static List<MediaItem> tracks(Context context,JSONObject list)throws JSONException{return tracks(context,list,new HashSet<>());}
    private static List<MediaItem> tracks(Context context,JSONObject list,Set<String> visited)throws JSONException {
        if(!visited.add(list.getString("id")))throw new IllegalArgumentException("Playlist sources cannot refer to each other in a loop");
        List<MediaItem> library=LibraryStore.read(context),songs=new ArrayList<>();
        if(!list.optBoolean("smart")){JSONArray ids=list.optJSONArray("songs");if(ids!=null)for(int i=0;i<ids.length();i++)for(MediaItem song:library)if(song.mediaId.equals(ids.getString(i))){songs.add(song);break;}return songs;}
        JSONArray sources=list.optJSONArray("sources");if(sources==null){sources=new JSONArray();if(!list.optString("source").isEmpty())sources.put(list.optString("source"));}
        Set<String> added=new HashSet<>();
        for(int sourceIndex=0;sourceIndex<sources.length();sourceIndex++){
            String source=sources.optString(sourceIndex);List<MediaItem> selected=new ArrayList<>();
            if(source.equals("all"))selected.addAll(library);
            else if(source.equals("selection")){for(MediaItem song:library)if(PhoneSmartRules.contains(list.optJSONArray("songs"),song.mediaId))selected.add(song);}
            else if(source.startsWith("folder:")){String folder=source.substring(7).replace('\\','/');SharedPreferences folders=context.getSharedPreferences("phone-folders",0);for(MediaItem song:library){String path=folders.getString(song.mediaId,"").replace('\\','/');if(path.equals(folder)||path.startsWith(folder.endsWith("/")?folder:folder+"/"))selected.add(song);}}
            else {JSONArray lists=read(context);for(int i=0;i<lists.length();i++){JSONObject candidate=lists.getJSONObject(i);if(source.equals(candidate.getString("id"))){selected.addAll(tracks(context,candidate,new HashSet<>(visited)));break;}}}
            for(MediaItem song:selected)if(added.add(song.mediaId)&&PhoneSmartRules.matches(context,song,list))songs.add(song);
        }
        if(songs.size()<2)return songs;
        String style=list.optString("style","Steady");double target=list.optDouble("energy",-1);if(target<0)target=songs.stream().mapToDouble(song->energy(context,song)).average().orElse(.5);
        Map<String,Double> tempos=new HashMap<>(),energies=new HashMap<>();Map<String,String> keys=new HashMap<>();android.content.SharedPreferences details=context.getSharedPreferences("phone-track-details",0);for(MediaItem song:songs){tempos.put(song.mediaId,bpm(context,song));energies.put(song.mediaId,energy(context,song));keys.put(song.mediaId,details.getString(song.mediaId+":key",DjEngine.preparation(song).optString("harmonic_key","")));}
        final double desired=target;Comparator<MediaItem> start=Comparator.comparingDouble(song->Math.abs(energies.get(song.mediaId)-desired)-(PhoneSmartRules.contains(list.optJSONArray("preferred"),song.mediaId)?.15:0));songs.sort(start);String recognition=list.optString("recognition_song");for(int i=0;i<songs.size();i++)if(songs.get(i).mediaId.equals(recognition)){songs.add(0,songs.remove(i));break;}
        List<MediaItem> order=new ArrayList<>();order.add(songs.remove(0));
        while(!songs.isEmpty()){MediaItem previous=order.get(order.size()-1);double best=Double.MAX_VALUE;int selected=0;for(int i=0;i<songs.size();i++){MediaItem next=songs.get(i);double a=tempos.get(previous.mediaId),b=tempos.get(next.mediaId),e=energies.get(next.mediaId),last=energies.get(previous.mediaId);double tempo=a>0&&b>0?Math.min(Math.abs(Math.log(b/a)),Math.min(Math.abs(Math.log(b*2/a)),Math.abs(Math.log(b/2/a)))):0;double desiredNow=desired;JSONObject progression=list.optJSONObject("energy_progression");if(progression!=null){double fraction=Math.min(1.,order.size()/(double)Math.max(1,progression.optInt("transitions",4)));desiredNow=desired+(progression.optDouble("target",desired)-desired)*fraction;if(!progression.optBoolean("hold",true)&&fraction>=1)desiredNow=desired;}double score=tempo*3+Math.abs(e-last)*2+Math.abs(e-desiredNow)-(PhoneSmartRules.contains(list.optJSONArray("preferred"),next.mediaId)?.15:0)+harmonicPenalty(keys.get(previous.mediaId),keys.get(next.mediaId));if(style.equals("Hype"))score+=Math.max(0,last-e);if(style.equals("Chill"))score+=Math.max(0,e-last);if(score<best){best=score;selected=i;}}order.add(songs.remove(selected));}return order;
    }
    private static double harmonicPenalty(String a,String b){a=a.toUpperCase(Locale.ROOT);b=b.toUpperCase(Locale.ROOT);if(!a.matches("(1[0-2]|[1-9])[AB]")||!b.matches("(1[0-2]|[1-9])[AB]"))return 0;int left=Integer.parseInt(a.substring(0,a.length()-1)),right=Integer.parseInt(b.substring(0,b.length()-1));int distance=Math.min(Math.abs(left-right),12-Math.abs(left-right));return distance==0||distance==1&&a.charAt(a.length()-1)==b.charAt(b.length()-1)?0:.4;}
}
