package cloud.gitwide.ampverse;
import android.content.*;
import android.os.*;
import androidx.test.core.app.ActivityScenario;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import org.junit.*;
import org.junit.runner.RunWith;
import org.json.*;
import static org.junit.Assert.*;

@RunWith(AndroidJUnit4.class)
public final class ModuleIntegrationTest {
 @Test public void coreContractAndCompanionSignaturesMatch()throws Exception{
  Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();
  JSONObject hello=ModuleRpc.call(c,"hello",null);assertEquals(1,hello.getInt("api"));assertEquals(BuildConfig.VERSION_CODE,hello.getInt("core"));
  for(String id:Modules.IDS)assertEquals("cloud.gitwide.ampverse."+id,Modules.installed(c,id).packageName);
 }
 @Test public void pagedLibraryAndInvalidCommandsAreSafe()throws Exception{
  Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();
  JSONObject page=ModuleRpc.call(c,"library",new JSONObject().put("offset",0).put("query",""));assertTrue(page.getJSONArray("items").length()<=50);assertTrue(page.getInt("total")>=page.getJSONArray("items").length());
  try{ModuleRpc.call(c,"module_read",new JSONObject().put("key","../../settings"));fail();}catch(IllegalStateException expected){assertTrue(expected.getMessage().contains("Invalid"));}
 }
 @Test public void sharedModuleSettingsSurviveWithoutPrivateFileAccess()throws Exception{
  Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();String key="lab.integration_test";
  String old=ModuleRpc.call(c,"module_read",new JSONObject().put("key",key)).getString("value");
  try{ModuleRpc.call(c,"module_write",new JSONObject().put("key",key).put("value","{\"revision\":7}"));assertEquals("{\"revision\":7}",ModuleRpc.call(c,"module_read",new JSONObject().put("key",key)).getString("value"));}
  finally{ModuleRpc.call(c,"module_write",new JSONObject().put("key",key).put("value",old));}
 }
 @Test public void installerRejectsWrongIdentityAndAcceptsMatchingSignedModule()throws Exception{
  Context c=InstrumentationRegistry.getInstrumentation().getTargetContext();android.content.pm.PackageInfo info=Modules.installed(c,"dj");
  java.io.File apk=new java.io.File(info.applicationInfo.sourceDir);String signer=AppUpdate.hex(java.security.MessageDigest.getInstance("SHA-256").digest(info.signingInfo.getApkContentsSigners()[0].toByteArray()));
  Modules.validate(c,apk,info.packageName,info.getLongVersionCode(),signer);
  try{Modules.validate(c,apk,info.packageName,info.getLongVersionCode(),"0".repeat(64));fail();}catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("signing"));}
  try{Modules.validate(c,apk,"cloud.gitwide.ampverse.lab",info.getLongVersionCode(),signer);fail();}catch(java.io.IOException expected){assertTrue(expected.getMessage().contains("Invalid"));}
 }
}
