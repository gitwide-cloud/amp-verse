package cloud.gitwide.ampverse;
import java.io.IOException;
import java.net.Proxy;
import java.util.concurrent.TimeUnit;
import okhttp3.*;
/** Exact-resource credentials: no redirects, proxies, or headers on ordinary internet audio. */
final class PrivateLibraryHttp {
 static OkHttpClient create(String baseAddress,String password,String server,String library,String targetAddress){
  HttpUrl base=HttpUrl.get(baseAddress),target=HttpUrl.get(targetAddress);
  return new OkHttpClient.Builder().proxy(Proxy.NO_PROXY).followRedirects(false).followSslRedirects(false)
   .connectTimeout(5,TimeUnit.SECONDS).readTimeout(20,TimeUnit.SECONDS)
   .addInterceptor(chain->{
    if(server.isEmpty()||library.isEmpty()||!base.scheme().equals(target.scheme())
     ||!base.host().equals(target.host())||base.port()!=target.port()||!target.username().isEmpty()||!target.password().isEmpty()
     ||!target.encodedPath().equals("/api/player/audio")||!library.equals(target.queryParameter("library_id"))
     ||!target.equals(chain.request().url()))throw new IOException("Private library source left its selected authority");
    return chain.proceed(chain.request().newBuilder().header("x-amp-password",password).header("x-amp-server",server).header("x-amp-library",library).build());
   }).build();
 }
}
