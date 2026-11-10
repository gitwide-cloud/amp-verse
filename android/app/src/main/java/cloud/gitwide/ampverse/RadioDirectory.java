package cloud.gitwide.ampverse;
import org.json.*;
import java.net.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
final class RadioDirectory {
    static JSONArray search(String query)throws Exception {
        Exception last=null;
        for(String host:new String[]{"de1.api.radio-browser.info","nl1.api.radio-browser.info"}){
            HttpURLConnection connection=null;
            try{
                URL url=new URL("https://"+host+"/json/stations/search?name="+URLEncoder.encode(query,"UTF-8")+"&limit=40&hidebroken=true");connection=(HttpURLConnection)url.openConnection();connection.setConnectTimeout(5000);connection.setReadTimeout(8000);connection.setInstanceFollowRedirects(false);connection.setRequestProperty("User-Agent","AMPVerse/0.1");
                if(connection.getResponseCode()!=200)throw new IOException("Radio directory unavailable");
                try(InputStream input=connection.getInputStream();ByteArrayOutputStream output=new ByteArrayOutputStream()){byte[] buffer=new byte[4096];int n;while((n=input.read(buffer))!=-1){if(output.size()+n>1024*1024)throw new IOException("Radio results too large");output.write(buffer,0,n);}return new JSONArray(output.toString(StandardCharsets.UTF_8.name()));}
            }catch(Exception error){last=error;}finally{if(connection!=null)connection.disconnect();}
        }
        throw new IOException("Could not search broadcast stations",last);
    }
}
