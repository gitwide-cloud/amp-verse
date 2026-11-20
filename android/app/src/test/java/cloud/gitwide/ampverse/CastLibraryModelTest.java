package cloud.gitwide.ampverse;
import org.json.*;
import org.junit.Test;
import static org.junit.Assert.*;
public class CastLibraryModelTest {
 @Test public void playbackUsesStablePathsAndPreservesOrderAndDuplicates()throws Exception {
  JSONArray paths=new JSONArray().put("Album B/曲目.mp3").put("Old missing.flac").put("Album B/曲目.mp3");
  JSONObject request=CastLibraryModel.play("library-A",paths,false);
  assertEquals("library-A",request.getString("library_id"));assertEquals(paths.toString(),request.getJSONArray("tracks").toString());assertFalse(request.getBoolean("server_speakers"));assertFalse(request.has("ids"));assertFalse(request.getBoolean("shuffle"));
  assertTrue(CastLibraryModel.play("library-A",paths,true).getBoolean("server_speakers"));
 }
 @Test public void unsafeOrEmptyPlaybackDoesNotBecomeARequest()throws Exception {
  for(String path:new String[]{"../a.mp3","a/../b.mp3","a//b.mp3","a\\b.mp3","/a.mp3","C:/a.mp3","a/./b.mp3",""}){
   try{CastLibraryModel.play("L",new JSONArray().put(path),false);fail(path);}catch(JSONException expected){}
  }
  try{CastLibraryModel.play("L",new JSONArray(),false);fail();}catch(JSONException expected){}
 }
 @Test public void smartEditsPreserveCompleteRulesAndUnknownFieldsWithoutMutatingDraft()throws Exception {
  JSONObject document=new JSONObject().put("name","Old").put("style","steady").put("target_energy",5).put("companion_available",true).put("source_root_hint","private-server-hint").put("allowed",new JSONArray().put("song_missing")).put("energy_progression",new JSONObject().put("mode","rise")).put("future_rule",new JSONObject().put("retain",true));
  JSONObject envelope=new JSONObject().put("kind","smart").put("document",document);String before=envelope.toString();
  JSONObject edited=CastLibraryModel.edited(envelope,"Renamed","hype","7");
  assertEquals(before,envelope.toString());assertEquals("Renamed",edited.getString("name"));assertTrue(edited.getBoolean("companion_available"));assertEquals("private-server-hint",edited.getString("source_root_hint"));assertEquals(document.getJSONArray("allowed").toString(),edited.getJSONArray("allowed").toString());assertTrue(edited.getJSONObject("future_rule").getBoolean("retain"));assertEquals("rise",edited.getJSONObject("energy_progression").getString("mode"));assertEquals(7,edited.getDouble("target_energy"),0);
 }
 @Test public void normalRenameRetainsUnavailableReferencesAndExtensions()throws Exception {
  JSONObject document=new JSONObject().put("name","Old").put("version",1).put("tracks",new JSONArray().put(new JSONObject().put("relative_path","missing.mp3")).put(new JSONObject().put("relative_path","missing.mp3"))).put("future",5);
  JSONObject renamed=CastLibraryModel.edited(new JSONObject().put("kind","normal").put("document",document),"New","","");
  assertEquals(document.getJSONArray("tracks").toString(),renamed.getJSONArray("tracks").toString());assertEquals(5,renamed.getInt("future"));assertEquals("Old",document.getString("name"));
 }
 @Test public void legacyServerCannotReceiveNewPlayerContract()throws Exception {
  assertFalse(CastLibraryModel.supports(new JSONObject()));assertFalse(CastLibraryModel.supports(new JSONObject().put("capabilities",new JSONObject().put("player_queue",1))));
  assertTrue(CastLibraryModel.supports(new JSONObject().put("capabilities",new JSONObject().put("player_queue",1).put("player_track_refs","library_path_v1"))));
 }
 @Test public void openLanNeedsNoCodeButOptionalProtectedModeStillRequiresIt()throws Exception {
  CastLibraryModel.requireAccess(new JSONObject().put("password_required",false),"");
  CastLibraryModel.requireAccess(new JSONObject().put("password_required",true),"configured-code");
  try{CastLibraryModel.requireAccess(new JSONObject().put("password_required",true),"");fail();}catch(JSONException expected){assertTrue(expected.getMessage().contains("requires"));}
 }
 @Test public void windowsCatalogSeparatorsBecomeTheSamePortableReferences()throws Exception {
  assertEquals("Album/曲.mp3",CastLibraryModel.catalogPath("Album\\曲.mp3"));
  try{CastLibraryModel.catalogPath("Album\\..\\outside.mp3");fail();}catch(JSONException expected){}
  assertEquals("Album/曲.mp3",CastLibraryModel.catalogPath("Album/曲.mp3"));
 }
 @Test public void savedLongNormalPlayUsesIdentityWithoutExpandingMissingReferences()throws Exception {
  JSONArray refs=new JSONArray();for(int i=0;i<1500;i++)refs.put(new JSONObject().put("relative_path",i==10?"missing.mp3":"song"+i+".mp3"));JSONObject envelope=new JSONObject().put("kind","normal").put("document_id","normal_Legacy.list").put("document",new JSONObject().put("tracks",refs));String original=envelope.toString();
  JSONObject request=CastLibraryModel.savedPlay("library-A",envelope,true);assertEquals("normal_Legacy.list",request.getString("playlist_id"));assertFalse(request.has("tracks"));assertTrue(request.toString().length()<200);assertTrue(request.getBoolean("server_speakers"));assertEquals(original,envelope.toString());
 }
 @Test public void orderedPhoneSnapshotSkipsOnlyReportedUnavailablePlaybackMembersAndKeepsSavedRefs()throws Exception {
  JSONObject envelope=new JSONObject().put("kind","normal").put("document_id","normal_mix").put("document",new JSONObject().put("tracks",new JSONArray().put("missing.mp3").put("A.mp3").put("A.mp3")));String original=envelope.toString();
  JSONObject data=new JSONObject().put("library_id","library-A").put("playlist_id","normal_mix").put("selection_mode","ordered").put("unavailable",1).put("tracks",new JSONArray().put(new JSONObject().put("path","Album\\A.mp3")).put(new JSONObject().put("path","Album/A.mp3")));
  CastLibraryModel.Snapshot snapshot=CastLibraryModel.savedSnapshot("library-A",envelope,data);assertEquals(1,snapshot.unavailable);assertFalse(snapshot.smart);assertEquals("[\"Album/A.mp3\",\"Album/A.mp3\"]",snapshot.paths.toString());assertEquals(original,envelope.toString());
  for(String field:new String[]{"library_id","playlist_id","selection_mode"}){JSONObject wrong=new JSONObject(data.toString()).put(field,"changed");try{CastLibraryModel.savedSnapshot("library-A",envelope,wrong);fail(field);}catch(JSONException expected){}}
 }
}
