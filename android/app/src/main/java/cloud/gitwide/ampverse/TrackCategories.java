package cloud.gitwide.ampverse;

import java.util.Locale;
import org.json.JSONArray;
import org.json.JSONObject;

/** Declared categories only. Unmarked does not establish that a song is human-made. */
final class TrackCategories {
    private TrackCategories() {}
    static String key(String field) {
        switch(field) {
            case "AiGenerated": return "ai_generated";
            case "Acappella": return "acappella";
            case "Instrumental": return "instrumental";
            case "Clip": return "clip";
            default: return "";
        }
    }
    static boolean marked(JSONObject preparation, String field) {
        String key = key(field);
        if (key.isEmpty()) return false;
        JSONObject values = PhoneSmartRules.extensions(preparation);
        Object declared = PhoneSmartRules.object(values, "track_annotations").opt(key);
        if (declared instanceof Boolean) return (Boolean) declared;
        JSONArray embedded=PhoneSmartRules.object(values,"embedded_tags").optJSONArray("AMPVerseCategories");
        if(embedded!=null&&embedded.length()==1&&embedded.opt(0) instanceof String) {
            String text=(String)embedded.opt(0);
            if(text.getBytes(java.nio.charset.StandardCharsets.UTF_8).length<=4096)try {
                JSONObject map=new JSONObject(text);boolean valid=map.length()<=32;
                java.util.Iterator<String> keys=map.keys();
                while(keys.hasNext()){String name=keys.next();valid &= name.getBytes(java.nio.charset.StandardCharsets.UTF_8).length<=64&&map.opt(name) instanceof Boolean;}
                if(valid&&map.opt(key) instanceof Boolean)return (Boolean)map.opt(key);
            }catch(org.json.JSONException ignored) { /* Invalid declarations never establish a category. */ }
        }
        java.util.List<String> genres = new java.util.ArrayList<>();
        Object source = values.has("genre") ? values.opt("genre") : values.opt("embedded_genres");
        if(source instanceof String)genres.add((String)source);
        else if(source instanceof JSONArray) {
            JSONArray array=(JSONArray)source;
            for(int i=0;i<array.length();i++)if(array.opt(i) instanceof String)genres.add((String)array.opt(i));
        }
        java.util.List<String> pieces=new java.util.ArrayList<>();
        for(String genre:genres)java.util.Collections.addAll(pieces,genre.split("[;/\\x00]"));
        for (String genre : pieces) {
            String normalized = genre.toLowerCase(Locale.ROOT).replaceAll("[^\\p{L}\\p{N}]", "");
            switch(field) {
                case "AiGenerated": if(normalized.equals("aigenerated") || normalized.equals("aimusic"))return true; break;
                case "Acappella": if(normalized.equals("acappella") || normalized.equals("acapella"))return true; break;
                case "Instrumental": if(normalized.equals("instrumental"))return true; break;
                case "Clip": if(normalized.equals("clip"))return true; break;
                default: break;
            }
        }
        return false;
    }
    static int requiredVersion(JSONObject document) {
        int version = 1;
        JSONArray filters = document.optJSONArray("filters");
        if (filters == null) return version;
        for (int i=0;i<filters.length();i++) {
            JSONObject filter=filters.optJSONObject(i);
            if(filter==null)continue;
            String field=filter.optString("field");
            version=Math.max(version,!key(field).isEmpty()?4:field.equals("Clean")?3:2);
        }
        return version;
    }
}
