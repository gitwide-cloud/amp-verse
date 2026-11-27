package cloud.gitwide.ampverse;

import java.util.*;
import org.json.*;

/** Mirrors amp-core seasonal content labels. Never infers from date or artist. */
final class SeasonalMusic {
    static final String[] HOLIDAYS={"Christmas","Halloween","Thanksgiving","New Year","Valentine’s Day","Easter","Hanukkah","Kwanzaa","Diwali","Ramadan","Eid","St Patrick’s Day","Independence Day"};
    static final String[] SEASONS={"Spring","Summer","Autumn","Winter"};
    static final String[][] ALIASES={{"christmas","xmas","noel","navidad"},{"halloween"},{"thanksgiving"},{"new year","new years"},{"valentine","valentines"},{"easter"},{"hanukkah","chanukah","hanukah"},{"kwanzaa"},{"diwali","deepavali"},{"ramadan","ramazan"},{"eid"},{"st patricks day","saint patricks day"},{"independence day","fourth of july","4th of july"}};
    static final java.util.regex.Pattern SEPARATORS=java.util.regex.Pattern.compile("[^\\p{L}\\p{N}]+");
    static String normalize(String s){return SEPARATORS.matcher(s.toLowerCase(Locale.ROOT).replace("'","").replace("’","")).replaceAll(" ").trim();}
    static boolean phrase(String value,String term){return (" "+value+" ").contains(" "+term+" ");}
    static String season(String value){switch(value){case "spring":return "Spring";case "summer":return "Summer";case "autumn":case "fall":return "Autumn";case "winter":return "Winter";default:return null;}}
    static void classifyHoliday(String value,Set<String> out){for(int i=0;i<HOLIDAYS.length;i++)for(String alias:ALIASES[i])if(phrase(value,alias)){out.add(HOLIDAYS[i]);break;}}
    static List<String> values(JSONObject preparation,String field){
        JSONObject x=PhoneSmartRules.extensions(preparation),tags=PhoneSmartRules.object(x,"embedded_tags");
        String key=field.equals("Holiday")?"holiday":"season";
        List<String> explicit=new ArrayList<>();PhoneSmartRules.strings(x.opt(key),explicit);PhoneSmartRules.strings(tags.opt(field),explicit);
        Set<String> out=new LinkedHashSet<>();
        for(String raw:explicit)for(String part:raw.split("[;/\\x00]")){String value=normalize(part);if(field.equals("Holiday"))classifyHoliday(value,out);else{String name=season(value);if(name!=null)out.add(name);}}
        List<String> rawGenres=new ArrayList<>(),labels=new ArrayList<>();PhoneSmartRules.strings(x.has("genre")?x.opt("genre"):x.opt("embedded_genres"),rawGenres);
        for(String raw:rawGenres)for(String part:raw.split("[;/\\x00]"))labels.add(part);
        PhoneSmartRules.strings(x.has("album")?x.opt("album"):tags.opt("AlbumTitle"),labels);
        if(field.equals("Holiday"))PhoneSmartRules.strings(x.has("title")?x.opt("title"):tags.opt("TrackTitle"),labels);
        for(String raw:labels){String value=normalize(raw);if(field.equals("Holiday")){classifyHoliday(value,out);continue;}
            for(String name:SEASONS){String term=name.toLowerCase(Locale.ROOT);boolean match=value.equals(term);for(String marker:new String[]{"music","hits","songs","collection","playlist","anthems","vibes"})match|=phrase(value,term)&&phrase(value,marker);if(match)out.add(name);}
            if(value.equals("fall")||phrase(value,"fall music")||phrase(value,"fall hits"))out.add("Autumn");
        }
        return new ArrayList<>(out);
    }
}
