package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;
import okhttp3.HttpUrl;
import java.net.InetAddress;
public final class LanAudioPolicyTest {
    @Test public void manualPauseFocusAndDjOwnershipBlockRecovery(){
        assertTrue(LiveRecoveryPolicy.allowed(true,true,false,false));
        assertFalse(LiveRecoveryPolicy.allowed(true,false,false,false));
        assertFalse(LiveRecoveryPolicy.allowed(true,true,true,false));
        assertFalse(LiveRecoveryPolicy.allowed(true,true,false,true));
        assertFalse(LiveRecoveryPolicy.allowed(false,true,false,false));
        assertFalse(LiveRecoveryPolicy.preparationExpired(1000,10999));assertTrue(LiveRecoveryPolicy.preparationExpired(1000,11000));
        assertFalse(LiveRecoveryPolicy.preparationExpired(0,30000));
        assertFalse(LiveRecoveryPolicy.stalled(1000,6999));assertTrue(LiveRecoveryPolicy.stalled(1000,7000));
        assertFalse(LiveRecoveryPolicy.behind(5001,15000,10000));assertTrue(LiveRecoveryPolicy.behind(5001,21000,10000));
    }
    @Test public void focusRestorationReschedulesAFailedStreamButManualPauseDoesNot(){
        boolean failed=true;
        assertFalse(LiveRecoveryPolicy.retryError(LiveRecoveryPolicy.allowed(true,true,true,false),failed,false));
        // The pending timer can expire during focus loss; the watchdog must
        // reconsider the unchanged error when playback permission returns.
        assertTrue(LiveRecoveryPolicy.retryError(LiveRecoveryPolicy.allowed(true,true,false,false),failed,false));
        assertFalse(LiveRecoveryPolicy.retryError(LiveRecoveryPolicy.allowed(true,false,false,false),failed,false));
        assertFalse(LiveRecoveryPolicy.retryError(LiveRecoveryPolicy.allowed(true,true,false,true),failed,false));
        assertFalse(LiveRecoveryPolicy.retryError(true,false,false));
    }
    @Test public void hlsRequestsCannotLeaveTheirIdentityScope(){
        HttpUrl master=HttpUrl.get("http://192.168.1.93:8787/api/audio/hls/master.m3u8");
        assertTrue(StationMediaSourceFactory.sameStationResource(master,HttpUrl.get("http://192.168.1.93:8787/api/audio/hls/epoch/resilient/segment-4.m4s")));
        assertFalse(StationMediaSourceFactory.sameStationResource(master,HttpUrl.get("http://192.168.1.94:8787/api/audio/hls/master.m3u8")));
        assertFalse(StationMediaSourceFactory.sameStationResource(master,HttpUrl.get("https://192.168.1.93:8787/api/audio/hls/master.m3u8")));
        assertFalse(StationMediaSourceFactory.sameStationResource(master,HttpUrl.get("http://192.168.1.93:8788/api/audio/hls/master.m3u8")));
        assertFalse(StationMediaSourceFactory.sameStationResource(master,HttpUrl.get("http://192.168.1.93:8787/api/start")));
        assertFalse(StationMediaSourceFactory.sameStationResource(master,HttpUrl.get("http://user:secret@192.168.1.93:8787/api/audio/hls/master.m3u8")));
    }
    @Test public void advertisedAddressesArePrivateBoundedAndNotCredentials()throws Exception{
        InetAddress peer=InetAddress.getByName("192.168.1.227");
        assertTrue(CastDiscovery.safePath("http://192.168.1.93:8787",8787,peer));
        assertFalse(CastDiscovery.safePath("http://8.8.8.8:8787",8787,peer));
        assertFalse(CastDiscovery.safePath("http://127.0.0.1:8787",8787,peer));
        assertFalse(CastDiscovery.safePath("http://0.0.0.0:8787",8787,peer));
        assertFalse(CastDiscovery.safePath("http://192.168.1.93:9999",8787,peer));
        assertFalse(CastDiscovery.safePath("http://user:secret@192.168.1.93:8787",8787,peer));
        assertFalse(CastDiscovery.safePath("http://192.168.1.93:8787?password=x",8787,peer));
        assertFalse(CastDiscovery.safePath("http://example.com:8787",8787,peer));
    }
}
