package cloud.gitwide.ampverse;

import android.content.*;
import androidx.media3.common.*;
import androidx.test.platform.app.InstrumentationRegistry;
import org.json.*;
import org.junit.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import static org.junit.Assert.*;

public class SmartPlaylistDeviceTest {
    private Context context;
    private File root;
    private String namespace;
    @Before public void setup() throws Exception {
        Context base=InstrumentationRegistry.getInstrumentation().getTargetContext();
        root=Files.createTempDirectory(base.getCacheDir().toPath(),"smart-rules-").toFile();namespace="smart-test-"+UUID.randomUUID();
        context=new ContextWrapper(base){@Override public File getFilesDir(){return root;}@Override public SharedPreferences getSharedPreferences(String name,int mode){return base.getSharedPreferences(namespace+name,mode);}};
    }
    @After public void cleanup() throws Exception {
        for(String name:new String[]{"phone-folders","phone-playlists","phone-track-details"})context.getSharedPreferences(name,0).edit().clear().commit();
        try(java.util.stream.Stream<Path> files=Files.walk(root.toPath())){files.sorted(Comparator.reverseOrder()).forEach(p->p.toFile().delete());}
    }
    private MediaItem item(String id,String preparation) throws Exception {
        File file=new File(root,"audio/"+id);file.getParentFile().mkdirs();Files.write(file.toPath(),new byte[]{1});
        CatalogStore.put(new File(root,"ampdj/catalog.ampdb"),"tracks/"+id+".json",preparation);
        return LibraryStore.item(file,id);
    }
    private JSONObject list() throws Exception{return new JSONObject("{\"id\":\"test\",\"smart\":true,\"source\":\"all\"}");}
    @Test public void editorShowsTouchChoicesAndCancelsWithoutSaving() throws Exception {
        Context real=InstrumentationRegistry.getInstrumentation().getTargetContext();String before=PhonePlaylists.read(real).toString();
        androidx.test.uiautomator.UiDevice device=androidx.test.uiautomator.UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
        device.wakeUp();device.executeShellCommand("wm dismiss-keyguard");
        try(androidx.test.core.app.ActivityScenario<MainActivity> scenario=androidx.test.core.app.ActivityScenario.launch(new Intent(real,MainActivity.class))){
            scenario.onActivity(activity->{try{activity.setShowWhenLocked(true);activity.setTurnScreenOn(true);activity.getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);PhoneLibraryPanel panel=new PhoneLibraryPanel(activity,new Appearance(activity),items->{});activity.setContentView(panel);java.lang.reflect.Method edit=PhoneLibraryPanel.class.getDeclaredMethod("edit",JSONObject.class);edit.setAccessible(true);edit.invoke(panel,new JSONObject("{\"id\":\"ui-draft-only\",\"name\":\"Test draft\",\"smart\":true,\"source\":\"all\"}"));}catch(Exception e){throw new RuntimeException(e);}});
            device.waitForIdle();device.takeScreenshot(new File(real.getExternalFilesDir(null),"smart-editor-main.png"));device.dumpWindowHierarchy(new File(real.getExternalFilesDir(null),"smart-editor-main.xml"));
            assertTrue(device.wait(androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.text("Choose music sources")),3000));
            androidx.test.uiautomator.UiScrollable scroll=new androidx.test.uiautomator.UiScrollable(new androidx.test.uiautomator.UiSelector().scrollable(true));
            if(device.findObject(androidx.test.uiautomator.By.text("Genres & filters"))==null)assertTrue(scroll.scrollTextIntoView("Genres & filters"));
            device.findObject(androidx.test.uiautomator.By.text("Genres & filters")).click();
            assertTrue(device.wait(androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.text("Smart Playlist options")),3000));
            device.takeScreenshot(new File(real.getExternalFilesDir(null),"smart-playlist-options.png"));
            device.findObject(androidx.test.uiautomator.By.text("Choose genres")).click();
            assertTrue(device.wait(androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.text("R&B")),2000));
            device.findObject(androidx.test.uiautomator.By.text("R&B")).click();device.pressBack();device.pressBack();device.pressBack();
            assertEquals(before,PhonePlaylists.read(real).toString());
        }
    }
    @Test public void syncedFiltersUseDesktopScalesAndExcludeUnknownValues() throws Exception {
        MediaItem song=item("known.wav","{\"bpm\":119,\"harmonic_key\":\"8A\",\"extensions\":{\"energy\":0.7,\"embedded_genres\":[\"Rhythm and Blues\"]}}");
        JSONObject rules=list().put("genres",new JSONArray("[\"R&B\"]")).put("filters",new JSONArray("[{\"type\":\"Number\",\"field\":\"Energy\",\"min\":7,\"max\":8},{\"type\":\"Text\",\"field\":\"Camelot\",\"terms\":\"7A;8a\",\"exact\":true}]"));
        assertTrue(PhoneSmartRules.matches(context,song,rules));assertEquals(.7,PhonePlaylists.energy(context,song),.001);
        assertFalse(PhoneSmartRules.matches(context,item("unknown.wav","{}"),rules));
        rules.put("filters",new JSONArray("[{\"type\":\"Number\",\"field\":\"TempoConfidence\",\"min\":0,\"max\":100}]"));assertFalse(PhoneSmartRules.matches(context,song,rules));
    }
    @Test public void realRustFlattenedPreparationSupportsGenresTagsAndAudioProperties() throws Exception {
        MediaItem song=item("rust-schema.wav","{\"cue_in\":0,\"cue_out\":120,\"bpm\":128,\"energy\":0.8,\"embedded_genres\":[\"Rock\"],\"artist\":\"Test artist\",\"audio_properties\":{\"bitrate\":256},\"energy_profile\":{\"sections\":[{\"energy\":0.9,\"name\":\"Peak\"}]}}");
        JSONObject rules=list().put("genres",new JSONArray("[\"Rock\"]")).put("filters",new JSONArray("[{\"type\":\"Text\",\"field\":\"Artist\",\"terms\":\"Test artist\",\"exact\":true},{\"type\":\"Number\",\"field\":\"Bitrate\",\"min\":200,\"max\":320},{\"type\":\"Flag\",\"field\":\"EnergyProfile\",\"value\":true}]"));
        assertTrue(PhoneSmartRules.matches(context,song,rules));assertEquals(.8,PhonePlaylists.energy(context,song),.001);
    }
    @Test public void allDesktopFilterFieldsHaveKnownTypesAndFlagSemantics() throws Exception {
        assertEquals(32,PhoneSmartRules.FIELDS.length);assertEquals(PhoneSmartRules.FIELDS.length,PhoneSmartRules.LABELS.length);
        MediaItem song=item("manual.wav","{\"manual_cues\":[{\"label\":\"Drop\",\"kind\":\"hot\"}],\"analysis\":{\"reviewed\":true},\"stems\":[{},{}]}");
        assertTrue(PhoneSmartRules.matches(context,song,list().put("filters",new JSONArray("[{\"type\":\"Flag\",\"field\":\"Reviewed\",\"value\":true},{\"type\":\"Number\",\"field\":\"StemCount\",\"min\":2,\"max\":4},{\"type\":\"Text\",\"field\":\"CueLabel\",\"terms\":\"dro\",\"exact\":false}]"))));
        assertFalse(PhoneSmartRules.matches(context,song,list().put("filters",new JSONArray("[{\"type\":\"Number\",\"field\":\"Energy\",\"min\":9,\"max\":2}]"))));
    }
    @Test public void multipleSourcesIncludeChildrenDeduplicateAndRespectExclusions() throws Exception {
        MediaItem a=item("a.wav","{}"),b=item("b.wav","{}"),c=item("c.wav","{}");LibraryStore.write(context,Arrays.asList(a,b,c));
        context.getSharedPreferences("phone-folders",0).edit().putString(a.mediaId,"Music/Rock").putString(b.mediaId,"Music/Rock/Live").putString(c.mediaId,"Music/Rocket").commit();
        PhonePlaylists.save(context,new JSONObject("{\"id\":\"normal\",\"name\":\"Normal\",\"smart\":false,\"songs\":[\"a.wav\",\"c.wav\"]}"));
        JSONObject rules=list().put("sources",new JSONArray("[\"folder:Music/Rock\",\"normal\"]")).put("excluded",new JSONArray("[\"c.wav\"]"));
        assertEquals(2,PhonePlaylists.tracks(context,rules).size());rules.put("sources",new JSONArray("[\"folder:Music/Rock\"]"));assertEquals(2,PhonePlaylists.tracks(context,rules).size());
        rules.put("sources",new JSONArray());assertTrue(PhonePlaylists.tracks(context,rules).isEmpty());rules.put("sources",new JSONArray("[\"selection\"]")).put("songs",new JSONArray().put(b.mediaId));assertEquals(b.mediaId,PhonePlaylists.tracks(context,rules).get(0).mediaId);
    }
    @Test public void hypeUsesApprovedCuesOnlyWhenRequested() throws Exception {
        MediaItem song=item("hype.wav","{\"cue_in\":2,\"cue_out\":60,\"manual_cues\":[{\"label\":\"Drop\",\"seconds\":16}]}");JSONObject rules=list().put("style","Hype");assertEquals(16000,PhoneSmartRules.entry(song,rules));rules.put("use_manual_cues",false);assertEquals(2000,PhoneSmartRules.entry(song,rules));rules.put("use_manual_cues",true).put("style","Steady");assertEquals(2000,PhoneSmartRules.entry(song,rules));
    }
    @Test public void repeatFallbackPrefersFreshMusicAndNeverReplaysSoundingSong() throws Exception {MediaItem a=item("repeat-a.wav","{}"),b=item("repeat-b.wav","{}"),c=item("repeat-c.wav","{}");List<MediaItem> source=Arrays.asList(a,b,c);assertEquals(c.mediaId,PhoneSmartRules.repeatCandidates(source,Arrays.asList(a.mediaId,b.mediaId),a.mediaId,5).get(0).mediaId);assertEquals(2,PhoneSmartRules.repeatCandidates(source,Arrays.asList(a.mediaId,b.mediaId,c.mediaId),a.mediaId,5).size());assertTrue(PhoneSmartRules.repeatCandidates(Collections.singletonList(a),Collections.singletonList(a.mediaId),a.mediaId,0).isEmpty());}
    @Test public void localOverridesTakePriorityAndRulesRoundTrip() throws Exception {
        MediaItem a=item("override.wav","{\"bpm\":100,\"extensions\":{\"energy\":0.2}}");
        context.getSharedPreferences("phone-track-details",0).edit().putFloat(a.mediaId+":bpm",125).putFloat(a.mediaId+":energy",.8f).commit();
        JSONObject rules=list().put("filters",new JSONArray("[{\"type\":\"Number\",\"field\":\"Bpm\",\"min\":120,\"max\":130}]")).put("name","Test");assertTrue(PhoneSmartRules.matches(context,a,rules));assertEquals(.8,PhonePlaylists.energy(context,a),.001);
        PhonePlaylists.save(context,rules);JSONArray saved=PhonePlaylists.read(context);assertEquals(rules.toString(),saved.getJSONObject(saved.length()-1).toString());
    }
    @Test public void recognitionOnlyStartsIfEligibleAndLegacySourcesStillWork() throws Exception {
        MediaItem a=item("one.wav","{\"extensions\":{\"energy\":0.3}}"),b=item("two.wav","{\"extensions\":{\"energy\":0.9}}");LibraryStore.write(context,Arrays.asList(a,b));
        JSONObject rules=list().put("recognition_song",b.mediaId).put("energy",.3);assertEquals(b.mediaId,PhonePlaylists.tracks(context,rules).get(0).mediaId);
        rules.put("excluded",new JSONArray().put(b.mediaId));assertEquals(a.mediaId,PhonePlaylists.tracks(context,rules).get(0).mediaId);
    }
}
