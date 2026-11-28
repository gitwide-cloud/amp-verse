package cloud.gitwide.ampverse;

import static org.junit.Assert.*;
import org.json.JSONObject;
import org.junit.Test;

public class TrackCategoriesTest {
    @Test public void phoneMatchesTheSameDeclaredCategoryVectorsAsRust() throws Exception {
        try(java.io.InputStream in=getClass().getClassLoader().getResourceAsStream("track_categories.json")) {
            assertNotNull(in);
            java.io.ByteArrayOutputStream out=new java.io.ByteArrayOutputStream();
            byte[] bytes=new byte[2048];int n;while((n=in.read(bytes))!=-1)out.write(bytes,0,n);
            org.json.JSONArray cases=new JSONObject(out.toString("UTF-8")).getJSONArray("cases");
            String[] fields={"AiGenerated","Acappella","Instrumental","Clip"};
            for(int i=0;i<cases.length();i++)for(int f=0;f<fields.length;f++) {
                JSONObject sample=cases.getJSONObject(i);
                assertEquals(sample.toString(),sample.getJSONArray("expected").getBoolean(f),TrackCategories.marked(sample.getJSONObject("preparation"),fields[f]));
            }
        }
    }

    @Test public void explicitChoicesOverrideDeclaredGenresWithoutTitleGuessing() throws Exception {
        JSONObject p=new JSONObject("{\"genre\":\"Rock;Instrumental\",\"embedded_genres\":[\"AI generated\"]}");
        assertTrue(TrackCategories.marked(p,"Instrumental"));
        assertFalse(TrackCategories.marked(p,"AiGenerated"));
        p.put("track_annotations",new JSONObject("{\"instrumental\":false,\"clip\":true,\"ai_generated\":true}"));
        assertFalse(TrackCategories.marked(p,"Instrumental"));
        assertTrue(TrackCategories.marked(p,"Clip"));
        assertTrue(TrackCategories.marked(p,"AiGenerated"));
        assertFalse(TrackCategories.marked(new JSONObject("{\"title\":\"AI Instrumental Clip\"}"),"Clip"));
        assertTrue(TrackCategories.marked(new JSONObject("{\"embedded_genres\":[\"A capella\"]}"),"Acappella"));
        assertFalse(TrackCategories.marked(new JSONObject("{\"genre\":\"\",\"embedded_genres\":[\"Instrumental\"]}"),"Instrumental"));
    }
    @Test public void categoriesRequireTheNewDocumentVersion() throws Exception {
        assertEquals(1,TrackCategories.requiredVersion(new JSONObject()));
        assertEquals(2,TrackCategories.requiredVersion(new JSONObject("{\"filters\":[{\"field\":\"Bpm\"}]}")));
        assertEquals(3,TrackCategories.requiredVersion(new JSONObject("{\"filters\":[{\"field\":\"Clean\"}]}")));
        assertEquals(4,TrackCategories.requiredVersion(new JSONObject("{\"filters\":[{\"field\":\"Clean\"},{\"field\":\"Clip\"}]}")));
    }
}
