package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;
import java.util.*;

public class CastHostSelectionTest {
 private static CastHostSelection.Host host(){return new CastHostSelection.Host("http://192.168.1.78:8787","server-A","station-A");}
 @Test public void redrawRetainsSelectionAndProcessRestoreRequiresVerifiedIdentity(){
  CastHostSelection owner=new CastHostSelection(null);owner.select(host());
  CastHostSelection.Host screenA=owner.selected(),screenB=owner.selected();
  assertSame(screenA,screenB);assertEquals("server-A/station-A",screenB.identity());
  CastHostSelection restored=new CastHostSelection(new CastHostSelection.Host(screenA.base,screenA.server,screenA.station));
  assertTrue(restored.selected().matches("server-A","station-A"));
  assertNull(new CastHostSelection(new CastHostSelection.Host(screenA.base,"","")).selected());
 }
 @Test public void disconnectInvalidatesInFlightRestoreAndDoesNotRejoin(){
  CastHostSelection state=new CastHostSelection(host());long pending=state.generation();
  state.disconnect();assertFalse(state.current(pending));assertNull(state.selected());assertFalse(state.retryDue(Long.MAX_VALUE));
 }
 @Test public void hostSwitchInvalidatesOldScreenResults(){
  CastHostSelection state=new CastHostSelection(host());long pending=state.generation();
  state.select(new CastHostSelection.Host("http://192.168.1.79:8787","B","S"));
  assertFalse(state.current(pending));assertEquals("B/S",state.selected().identity());
 }
 @Test public void retriesBackOffAcrossViewRecreationAndHaveBoundedDelay(){
  CastHostSelection state=new CastHostSelection(host());long now=100;
  for(int i=0;i<20;i++){state.failed(now);assertFalse(state.retryDue(now));assertFalse(state.retryDue(now+1999));assertTrue(state.retryDue(now+60000));}
  assertNotNull(state.selected());state.recovered();assertTrue(state.retryDue(now));
 }
 @Test public void networkChangeWakesRetryButDuplicateCallbackDoesNotResetBackoff(){
  CastHostSelection state=new CastHostSelection(host());assertTrue(state.networkChanged("wifi-1"));
  state.failed(100);assertFalse(state.networkChanged("wifi-1"));assertFalse(state.retryDue(101));
  assertTrue(state.networkChanged("wifi-2"));assertTrue(state.retryDue(101));
 }
 @Test public void emptyDiscoveryRetainsKnownHostsAndSameIdentityUpdatesAddress(){
  CastHostSelection state=new CastHostSelection(host());
  CastDiscovery.Server first=new CastDiscovery.Server("AMP Radio","http://192.168.1.78:8787","server-A/station-A");
  assertEquals(1,state.merge(Arrays.asList(first)).size());
  assertSame(first,state.merge(Collections.emptyList()).get(0));
  CastDiscovery.Server moved=new CastDiscovery.Server("AMP Radio","http://192.168.1.80:8787",first.id);
  assertSame(moved,state.merge(Arrays.asList(moved)).get(0));assertEquals(first.url,state.selected().base);
 }
 @Test public void discoveryCacheAndDiscoveryRetryWorkAreBounded(){
  CastHostSelection state=new CastHostSelection(host());List<CastDiscovery.Server> found=new ArrayList<>();
  for(int i=0;i<100;i++)found.add(new CastDiscovery.Server("Station","http://192.168.1.80:8787","id"+i));
  assertEquals(16,state.merge(found).size());int discovery=0;
  for(int i=0;i<20;i++){state.failed(0);if(state.discoveryDue())discovery++;}
  assertEquals(3,discovery);
 }
}
