package cloud.gitwide.ampverse;

import org.json.JSONObject;
import java.net.HttpURLConnection;
import java.net.URL;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;

final class ReleaseCheck {
    static String check(String installed) {
        HttpURLConnection connection=null;
        try {
            connection=(HttpURLConnection)new URL("https://api.github.com/repos/gitwide-cloud/amp-verse/releases/latest").openConnection();
            connection.setConnectTimeout(5000);connection.setReadTimeout(5000);connection.setInstanceFollowRedirects(false);
            connection.setRequestProperty("Accept","application/vnd.github+json");connection.setRequestProperty("User-Agent","AMPVerse-Android");
            int code=connection.getResponseCode();
            if(code==404)return "No public release is available yet, or the repository requires sign-in. Open GitHub Releases to check.";
            if(code==403||code==429)return "GitHub is limiting update checks. Try later or open Releases.";
            if(code!=200)return "Update check unavailable (HTTP "+code+"). Your installed app still works offline.";
            try(InputStream input=connection.getInputStream();ByteArrayOutputStream body=new ByteArrayOutputStream()){
                byte[] bytes=new byte[8192];int size;while((size=input.read(bytes))!=-1){if(body.size()+size>1048576)throw new java.io.IOException("Response too large");body.write(bytes,0,size);}
                return describe(body.toString("UTF-8"),installed);
            }
        }catch(Exception error){return "Could not check for updates. Check your connection or open GitHub Releases.";}
        finally{if(connection!=null)connection.disconnect();}
    }
    static String describe(String json,String installed)throws Exception{
        JSONObject release=new JSONObject(json);String tag=release.getString("tag_name");
        int comparison=compare(tag,installed);
        if(comparison>0)return "New release: "+tag+". Open GitHub Releases to review and download.";
        if(comparison==0)return "You have the latest published version ("+tag+").";
        return "Installed "+installed+" is newer than the latest published release ("+tag+").";
    }
    static int compare(String left,String right){
        String[] a=left.replaceFirst("^[vV]","").split("\\."),b=right.replaceFirst("^[vV]","").split("\\.");
        if(a.length!=3||b.length!=3)throw new IllegalArgumentException("Unsupported version tag");
        for(int i=0;i<3;i++)if(!a[i].matches("[0-9]{1,8}")||!b[i].matches("[0-9]{1,8}"))throw new IllegalArgumentException("Unsupported version tag");
        for(int i=0;i<3;i++){int result=Integer.compare(Integer.parseInt(a[i]),Integer.parseInt(b[i]));if(result!=0)return result;}return 0;
    }
}
