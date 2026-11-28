package cloud.gitwide.ampverse;
import org.json.*;
import org.junit.Test;
import static org.junit.Assert.*;
public class UpdaterReleaseTest {
 private JSONObject release(String version,boolean pre)throws Exception{return new JSONObject().put("tag_name",version).put("draft",false).put("prerelease",pre).put("assets",new JSONArray().put(new JSONObject().put("name","amp-update.json")));}
 @Test public void numericRcOrderAndStablePromotion()throws Exception{
  JSONArray feed=new JSONArray().put(release("v1.1.0-rc.9",true)).put(release("v0.3.1",false)).put(release("v1.1.0-rc.11",true));
  assertEquals("v1.1.0-rc.11",ReleaseCheck.select(feed,"1.1.0-rc.10").getString("tag_name"));
  assertNull(ReleaseCheck.select(feed,"1.1.0"));
  assertEquals("v1.1.0",ReleaseCheck.select(new JSONArray().put(release("v1.1.0",false)),"1.1.0-rc.11").getString("tag_name"));
 }
 @Test public void incompleteDraftWrongChannelAndDowngradeAreIgnored()throws Exception{
  JSONArray feed=new JSONArray().put(release("v1.1.0-rc.12",true).put("draft",true)).put(release("v1.1.0-rc.13",true).put("assets",new JSONArray())).put(release("v1.2.0-beta.1",true)).put(release("v1.1.0-rc.15",false)).put(release("v1.1.0-rc.9",true));
  assertNull(ReleaseCheck.select(feed,"1.1.0-rc.10"));
 }
 @Test public void moduleCatalogAllowsSameReleaseAndSortsRcNumerically()throws Exception{
  JSONObject first=release("v1.1.0-rc.9",true).put("assets",new JSONArray().put(new JSONObject().put("name","amp-modules.json")));
  JSONObject latest=release("v1.1.0-rc.11",true).put("assets",new JSONArray().put(new JSONObject().put("name","amp-modules.json")));
  assertEquals("v1.1.0-rc.11",ReleaseCheck.newestWithAsset(new JSONArray().put(first).put(latest),"1.1.0-rc.11","amp-modules.json",false).getString("tag_name"));
  assertNull(ReleaseCheck.newestWithAsset(new JSONArray().put(latest),"1.1.0","amp-modules.json",false));
 }

}
