package cloud.gitwide.ampverse;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;
@RunWith(AndroidJUnit4.class)
public class CastAddressDeviceTest {
    @Test public void bindingAddressesCannotBeUsedAsRemoteDestinations()throws Exception {
        for(String address:new String[]{"localhost:8787","127.0.0.1:8787","127.0.0.2:8787","0.0.0.0:8787","http://[::]:8787","http://[::1]:8787"}){
            try{new CastClient(address,"");fail("Accepted an unreachable remote address: "+address);}
            catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("LAN address"));}
        }
        assertEquals("http://192.168.10.5:8877",new CastClient("192.168.10.5:8877","").base);
    }
}
