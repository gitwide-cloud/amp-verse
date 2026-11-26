package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;
import java.util.concurrent.TimeUnit;

public class CastHostReconnectTest {
 private static CastClient publicClient(CastLibraryApiTest.Fixture f)throws Exception {
  CastClient c=new CastClient("http://192.0.2.1","");
  java.lang.reflect.Field base=CastClient.class.getDeclaredField("base");base.setAccessible(true);base.set(c,"http://127.0.0.1:"+f.server.getLocalPort());return c;
 }
 @Test public void publicLanRestoresAfterRecreationWithoutCommandsOrPassword()throws Exception {
  try(CastLibraryApiTest.Fixture f=new CastLibraryApiTest.Fixture(CastLibraryApiTest.status("station-A"))){
   CastHostReconnect.Verified v=CastHostReconnect.restore("http://127.0.0.1:"+f.server.getLocalPort(),"server-A/station-A",null,base->publicClient(f));
   f.done.get(5,TimeUnit.SECONDS);assertEquals("server-A",v.status.getString("server_id"));assertEquals(1,f.requests.size());
   assertTrue(f.requests.get(0).startsWith("GET /api/status "));assertFalse(f.requests.get(0).contains("x-amp-password:"));
  }
 }
 @Test public void changedHostAtSavedAddressNeverReceivesCachedCredentialOrCommand()throws Exception {
  try(CastLibraryApiTest.Fixture f=new CastLibraryApiTest.Fixture(CastLibraryApiTest.status("station-B"))){
   CastClient memory=f.client();
   try{CastHostReconnect.restore(memory.base,"server-A/station-A",memory,base->publicClient(f));fail();}
   catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("identity changed"));}
   f.done.get(5,TimeUnit.SECONDS);assertEquals(1,f.requests.size());assertFalse(f.requests.get(0).contains("private-fixture-code"));assertTrue(f.requests.get(0).startsWith("GET /api/status "));
  }
 }
 @Test public void unchangedHostReusesMemoryAccessOnlyAfterPublicIdentityProbe()throws Exception {
  try(CastLibraryApiTest.Fixture f=new CastLibraryApiTest.Fixture(CastLibraryApiTest.status("station-A"),CastLibraryApiTest.status("station-A"),CastLibraryApiTest.status("station-A"))){
   CastClient memory=f.client();memory.connect();
   CastHostReconnect.Verified v=CastHostReconnect.restore(memory.base,"server-A/station-A",memory,base->publicClient(f));
   f.done.get(5,TimeUnit.SECONDS);assertSame(memory,v.client);assertFalse(f.requests.get(1).contains("x-amp-password:"));assertTrue(f.requests.get(2).contains("private-fixture-code"));
   for(String request:f.requests)assertTrue(request.startsWith("GET /api/status "));
  }
 }
 @Test public void changedAddressUsesPublicClientWithoutForwardingMemorySecret()throws Exception {
  try(CastLibraryApiTest.Fixture f=new CastLibraryApiTest.Fixture(CastLibraryApiTest.status("station-A"))){
   CastClient old=new CastClient("http://192.0.2.10:8787","private-old-code");
   CastHostReconnect.Verified v=CastHostReconnect.restore("http://127.0.0.1:"+f.server.getLocalPort(),"server-A/station-A",old,base->publicClient(f));
   f.done.get(5,TimeUnit.SECONDS);assertNotSame(old,v.client);assertFalse(f.requests.get(0).contains("private-old-code"));
  }
 }
}
