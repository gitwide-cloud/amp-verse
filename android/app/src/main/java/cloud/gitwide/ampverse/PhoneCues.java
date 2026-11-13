package cloud.gitwide.ampverse;
import android.content.*;
import androidx.media3.common.MediaItem;
import org.json.*;
import java.util.*;
final class PhoneCues {
 static long saved(JSONObject prep,int slot){for(String key:new String[]{"manual_cues","cues"}){JSONArray a=prep.optJSONArray(key);if(a!=null)for(int i=0;i<a.length();i++){JSONObject cue=a.optJSONObject(i);if(cue!=null&&cue.optInt("number",0)==slot+1){double value=cue.optDouble("seconds",Double.NaN);if(Double.isFinite(value)&&value>=0)return (long)(value*1000);}}}return -1;}
 static long seconds(Context c,MediaItem item,int slot){SharedPreferences p=c.getSharedPreferences("phone-cues",0);String key=item.mediaId+":"+slot;return p.contains(key)?p.getLong(key,0):saved(DjEngine.preparation(item),slot);}
 static void save(Context c,MediaItem item,int slot,long value){c.getSharedPreferences("phone-cues",0).edit().putLong(item.mediaId+":"+slot,value).putBoolean(item.mediaId+":manual:"+slot,true).apply();}
 static Runnable migration(Context c,String id,JSONObject previous,JSONObject incoming,JSONArray conflicts)throws JSONException{
  SharedPreferences p=c.getSharedPreferences("phone-cues",0);List<String> inherited=new ArrayList<>(),manual=new ArrayList<>();
  for(int slot=0;slot<8;slot++){String key=id+":"+slot;if(!p.contains(key))continue;long value=p.getLong(key,0),old=saved(previous,slot),next=saved(incoming,slot);if(p.getBoolean(id+":manual:"+slot,false)||old!=value){manual.add(id+":manual:"+slot);if(next!=old&&next!=value)conflicts.put(new JSONObject().put("field","hot_cue").put("media_id",id).put("slot",slot).put("phone",value).put("desktop",next));}else inherited.add(key);}
  return ()->{SharedPreferences.Editor edit=p.edit();for(String key:inherited)edit.remove(key);for(String key:manual)edit.putBoolean(key,true);edit.apply();};
 }
}
