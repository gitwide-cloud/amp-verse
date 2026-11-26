package cloud.gitwide.ampverse;

import android.content.Context;
import android.net.Uri;
import androidx.media3.common.MediaItem;
import androidx.media3.common.MimeTypes;
import androidx.media3.datasource.okhttp.OkHttpDataSource;
import androidx.media3.exoplayer.drm.DrmSessionManagerProvider;
import androidx.media3.exoplayer.hls.HlsMediaSource;
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory;
import androidx.media3.exoplayer.source.MediaSource;
import androidx.media3.exoplayer.upstream.LoadErrorHandlingPolicy;
import java.io.IOException;
import java.net.Proxy;
import java.util.concurrent.TimeUnit;
import okhttp3.HttpUrl;
import okhttp3.OkHttpClient;

/** Identity headers belong to one selected station, never ordinary internet radio. */
@androidx.annotation.OptIn(markerClass=androidx.media3.common.util.UnstableApi.class)
final class StationMediaSourceFactory implements MediaSource.Factory {
    private final DefaultMediaSourceFactory ordinary;
    private DrmSessionManagerProvider drm;
    private LoadErrorHandlingPolicy errors;
    StationMediaSourceFactory(Context context){ordinary=new DefaultMediaSourceFactory(new ProtectedCastSource.Factory(context));}
    static boolean adaptive(Uri uri){return uri!=null&&"/api/audio/hls/master.m3u8".equals(uri.getPath())&&identity(uri.getQueryParameter("server"))&&identity(uri.getQueryParameter("station"));}
    private static boolean identity(String value){return value!=null&&!value.isEmpty()&&value.length()<=256;}
    static void report(MediaItem item,String text){if(item!=null&&item.localConfiguration!=null&&adaptive(item.localConfiguration.uri)){Uri uri=item.localConfiguration.uri;StationAudioStatus.set(uri.getQueryParameter("server"),uri.getQueryParameter("station"),text);}}
    static boolean sameStationResource(HttpUrl master,HttpUrl resource){
        return master.scheme().equals(resource.scheme())&&master.host().equals(resource.host())&&master.port()==resource.port()
            &&resource.username().isEmpty()&&resource.password().isEmpty()&&resource.encodedPath().startsWith("/api/audio/hls/");
    }
    static OkHttpClient stationHttp(Uri selected){
        HttpUrl master=HttpUrl.get(selected.toString());
        final String server=selected.getQueryParameter("server"),station=selected.getQueryParameter("station");
        return new OkHttpClient.Builder().proxy(Proxy.NO_PROXY).followRedirects(false).followSslRedirects(false)
            .connectTimeout(1500,TimeUnit.MILLISECONDS).readTimeout(2500,TimeUnit.MILLISECONDS).callTimeout(4000,TimeUnit.MILLISECONDS)
            .addInterceptor(chain->{
                if(!sameStationResource(master,chain.request().url()))throw new IOException("Station playlist tried to leave its selected server");
                return chain.proceed(chain.request().newBuilder().header("x-amp-server",server).header("x-amp-station",station).build());
            }).build();
    }
    private MediaSource privateSource(MediaItem item){
        PrivateLibraryPlayback.Binding binding=PrivateLibraryPlayback.binding(item);
        String target=item.localConfiguration.uri.toString();
        OkHttpClient http= binding==null?new OkHttpClient.Builder().addInterceptor(chain->{throw new IOException("Private library access expired; reopen the song");}).build()
            :PrivateLibraryHttp.create(binding.base,binding.password,binding.server,binding.library,target);
        DefaultMediaSourceFactory source=new DefaultMediaSourceFactory(new OkHttpDataSource.Factory(http));
        if(drm!=null)source.setDrmSessionManagerProvider(drm);if(errors!=null)source.setLoadErrorHandlingPolicy(errors);
        return source.createMediaSource(item);
    }
    @Override public MediaSource createMediaSource(MediaItem item){
        if(item.localConfiguration!=null&&PrivateLibraryPlayback.owns(item))return privateSource(item);
        if(item.localConfiguration==null||!adaptive(item.localConfiguration.uri))return ordinary.createMediaSource(item);
        Uri uri=item.localConfiguration.uri;
        HlsMediaSource.Factory hls=new HlsMediaSource.Factory(new OkHttpDataSource.Factory(stationHttp(uri)));
        if(drm!=null)hls.setDrmSessionManagerProvider(drm);
        if(errors!=null)hls.setLoadErrorHandlingPolicy(errors);
        MediaItem live=item.buildUpon().setMimeType(MimeTypes.APPLICATION_M3U8)
            .setLiveConfiguration(new MediaItem.LiveConfiguration.Builder().setTargetOffsetMs(3000).setMinOffsetMs(2000).setMaxOffsetMs(5000)
                .setMinPlaybackSpeed(1f).setMaxPlaybackSpeed(1f).build()).build();
        report(item,"Adaptive AAC · connecting");
        return hls.createMediaSource(live);
    }
    @Override public int[] getSupportedTypes(){return ordinary.getSupportedTypes();}
    @Override public MediaSource.Factory setDrmSessionManagerProvider(DrmSessionManagerProvider value){drm=value;ordinary.setDrmSessionManagerProvider(value);return this;}
    @Override public MediaSource.Factory setLoadErrorHandlingPolicy(LoadErrorHandlingPolicy value){errors=value;ordinary.setLoadErrorHandlingPolicy(value);return this;}
}
