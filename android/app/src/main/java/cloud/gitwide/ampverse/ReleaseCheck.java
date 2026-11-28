package cloud.gitwide.ampverse;

import org.json.JSONObject;
import org.json.JSONArray;
import java.net.HttpURLConnection;
import java.net.URL;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;

final class ReleaseCheck {
    static String check(String installed) {
        HttpURLConnection connection=null;
        try {
            connection=(HttpURLConnection)new URL("https://api.github.com/repos/gitwide-cloud/amp-verse/releases?per_page=100").openConnection();
            connection.setConnectTimeout(5000);connection.setReadTimeout(5000);connection.setInstanceFollowRedirects(false);
            connection.setRequestProperty("Accept","application/vnd.github+json");connection.setRequestProperty("User-Agent","AMPVerse-Android");
            int code=connection.getResponseCode();
            if(code==404)return "No public release is available yet, or the repository requires sign-in. Open GitHub Releases to check.";
            if(code==403||code==429)return "GitHub is limiting update checks. Try later or open Releases.";
            if(code!=200)return "Update check unavailable (HTTP "+code+"). Your installed app still works offline.";
            try(InputStream input=connection.getInputStream();ByteArrayOutputStream body=new ByteArrayOutputStream()){
                byte[] bytes=new byte[8192];int size;while((size=input.read(bytes))!=-1){if(body.size()+size>4*1048576)throw new java.io.IOException("Response too large");body.write(bytes,0,size);}
                return describe(body.toString("UTF-8"),installed);
            }
        }catch(Exception error){return "Could not check for updates. Check your connection or open GitHub Releases.";}
        finally{if(connection!=null)connection.disconnect();}
    }
    static String describe(String json,String installed)throws Exception{
        JSONObject release=select(new JSONArray(json),installed);
        if(release==null)return "You have the latest compatible published update.";
        return "New update: "+release.getString("tag_name")+". Tap Update to install.";
    }
    static JSONObject select(JSONArray releases,String installed)throws Exception {
        int[] current=version(installed);JSONObject selected=null;
        for(int i=0;i<releases.length();i++){
            JSONObject item=releases.optJSONObject(i);if(item==null||item.optBoolean("draft",true))continue;
            String tag=item.optString("tag_name");int[] candidate;
            try{candidate=version(tag);}catch(IllegalArgumentException error){continue;}
            boolean pre=candidate[3]<2;
            if(item.optBoolean("prerelease",!pre)!=pre||candidate[3]<current[3]||compare(tag,installed)<=0)continue;
            JSONArray assets=item.optJSONArray("assets");boolean complete=false;
            if(assets!=null)for(int a=0;a<assets.length();a++){JSONObject asset=assets.optJSONObject(a);if(asset!=null&&asset.optString("name").equals("amp-update.json"))complete=true;}
            if(complete&&(selected==null||compare(tag,selected.getString("tag_name"))>0))selected=item;
        }return selected;
    }
    static boolean betaVersion(String version){
        return version!=null&&version.matches("[vV]?(?:0|[1-9][0-9]{0,7})\\.(?:0|[1-9][0-9]{0,7})\\.(?:0|[1-9][0-9]{0,7})-beta\\.[1-9][0-9]{0,7}");
    }
    static boolean rcVersion(String version){
        return version!=null&&version.matches("[vV]?(?:0|[1-9][0-9]{0,7})\\.(?:0|[1-9][0-9]{0,7})\\.(?:0|[1-9][0-9]{0,7})-rc\\.[1-9][0-9]{0,7}");
    }
    static boolean samePrereleaseChannel(String installed,String candidate){
        return betaVersion(installed)&&betaVersion(candidate)||rcVersion(installed)&&rcVersion(candidate);
    }
    private static int[] version(String value){
        java.util.regex.Matcher match=java.util.regex.Pattern.compile("[vV]?((?:0|[1-9][0-9]{0,7}))\\.((?:0|[1-9][0-9]{0,7}))\\.((?:0|[1-9][0-9]{0,7}))(?:-(beta|rc)\\.([1-9][0-9]{0,7}))?").matcher(value);
        if(!match.matches())throw new IllegalArgumentException("Unsupported version tag");
        String channel=match.group(4);int rank=channel==null?2:channel.equals("rc")?1:0;
        return new int[]{Integer.parseInt(match.group(1)),Integer.parseInt(match.group(2)),Integer.parseInt(match.group(3)),rank,channel==null?0:Integer.parseInt(match.group(5))};
    }
    static int compare(String left,String right){
        int[] a=version(left),b=version(right);
        for(int i=0;i<a.length;i++){int result=Integer.compare(a[i],b[i]);if(result!=0)return result;}
        return 0;
    }
}
