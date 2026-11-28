package cloud.gitwide.ampverse;
import org.json.*;
import java.io.*;
import java.net.*;
import java.util.concurrent.atomic.AtomicBoolean;

/** Per-download connections. Module cancellation never cancels a base-app update. */
final class ModuleDownloads {
    final AtomicBoolean cancelled=new AtomicBoolean();
    private volatile HttpURLConnection connection;
    void cancel(){cancelled.set(true);HttpURLConnection c=connection;if(c!=null)c.disconnect();}
    InputStream open(String address)throws Exception{
        String target=address;
        for(int redirects=0;redirects<6;redirects++){
            if(cancelled.get())throw new IOException("Download cancelled");URL url=new URL(target);String host=url.getHost();
            if(!url.getProtocol().equals("https")||!(host.equals("api.github.com")||host.equals("github.com")||host.endsWith(".githubusercontent.com")))throw new IOException("Use an official GitHub release download");
            HttpURLConnection c=(HttpURLConnection)url.openConnection();connection=c;c.setInstanceFollowRedirects(false);c.setConnectTimeout(10000);c.setReadTimeout(5000);c.setRequestProperty("User-Agent","AMPVerse-Modules");int status=c.getResponseCode();
            if(status>=300&&status<400){target=new URL(url,c.getHeaderField("Location")).toString();c.disconnect();continue;}
            if(status!=200){c.disconnect();throw new IOException(status==404?"No compatible module package is published yet":"GitHub returned "+status);}
            return new FilterInputStream(c.getInputStream()){@Override public void close()throws IOException{try{super.close();}finally{c.disconnect();}}};
        }throw new IOException("Too many release redirects");
    }
    String text(String url)throws Exception{try(InputStream in=open(url);ByteArrayOutputStream bytes=new ByteArrayOutputStream()){byte[] buffer=new byte[8192];int n;while((n=in.read(buffer))!=-1){if(bytes.size()+n>1048576)throw new IOException("Module catalog is too large");bytes.write(buffer,0,n);}return bytes.toString("UTF-8");}}
    JSONObject catalog()throws Exception{
        JSONArray releases=new JSONArray(text("https://api.github.com/repos/gitwide-cloud/amp-verse/releases?per_page=20"));
        JSONObject release=ReleaseCheck.newestWithAsset(releases,BuildConfig.VERSION_NAME,"amp-modules.json",false);
        if(release!=null){String tag=release.getString("tag_name");JSONArray assets=release.getJSONArray("assets");for(int j=0;j<assets.length();j++){JSONObject asset=assets.getJSONObject(j);if(asset.optString("name").equals("amp-modules.json")){String expected="https://github.com/gitwide-cloud/amp-verse/releases/download/"+tag+"/amp-modules.json";if(!asset.getString("browser_download_url").equals(expected))throw new IOException("Unexpected module catalog URL");return new JSONObject(text(expected));}}}
        throw new IOException("No compatible module catalog has been published on GitHub");
    }
}
