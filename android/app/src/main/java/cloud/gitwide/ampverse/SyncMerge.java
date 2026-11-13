package cloud.gitwide.ampverse;
import org.json.*;
import java.util.*;
/** Three-way field merge. Phone edits win conflicts; the desktop proposal remains reviewable. */
final class SyncMerge {
 static Object merge(Object base,Object phone,Object desktop,String field,JSONArray conflicts)throws JSONException{
  if(same(phone,base))return desktop;
  if(same(desktop,base)||same(phone,desktop))return phone;
  if(phone instanceof JSONObject&&desktop instanceof JSONObject){JSONObject a=base instanceof JSONObject?(JSONObject)base:new JSONObject(),b=(JSONObject)phone,c=(JSONObject)desktop,out=new JSONObject();Set<String> keys=new TreeSet<>();b.keys().forEachRemaining(keys::add);c.keys().forEachRemaining(keys::add);a.keys().forEachRemaining(keys::add);for(String k:keys){Object v=merge(a.opt(k),b.opt(k),c.opt(k),field+"."+k,conflicts);if(v!=null)out.put(k,v);}return out;}
  conflicts.put(new JSONObject().put("field",field).put("phone",phone==null?JSONObject.NULL:phone).put("desktop",desktop==null?JSONObject.NULL:desktop));return phone;
 }
 static boolean same(Object a,Object b)throws JSONException{
  if(a==b)return true;if(a==null||b==null)return false;
  if(a instanceof JSONObject&&b instanceof JSONObject){JSONObject x=(JSONObject)a,y=(JSONObject)b;if(x.length()!=y.length())return false;Iterator<String> keys=x.keys();while(keys.hasNext()){String key=keys.next();if(!y.has(key)||!same(x.opt(key),y.opt(key)))return false;}return true;}
  if(a instanceof JSONArray&&b instanceof JSONArray){JSONArray x=(JSONArray)a,y=(JSONArray)b;if(x.length()!=y.length())return false;for(int i=0;i<x.length();i++)if(!same(x.opt(i),y.opt(i)))return false;return true;}
  if(a instanceof Number&&b instanceof Number)return Double.compare(((Number)a).doubleValue(),((Number)b).doubleValue())==0;return a.equals(b);
 }
}
