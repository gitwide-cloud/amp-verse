package cloud.gitwide.ampverse;

import static org.junit.Assert.*;
import org.json.JSONObject;
import org.junit.Test;

public class ContentAdvisoryTest {
    @Test public void cleanFilterUsesDeclaredMetadataAndPreservesUnknown() throws Exception {
        assertEquals("Flag", PhoneSmartRules.kind("Clean"));
        JSONObject tags = new JSONObject("{\"embedded_tags\":{\"ParentalAdvisory\":[\"2\"]}}");
        assertTrue(PhoneSmartRules.flag(tags, "Clean"));
        tags.put("content_advisory", "Unknown");
        assertFalse(PhoneSmartRules.flag(tags, "Clean"));
        tags.put("content_advisory", "Explicit");
        assertFalse(PhoneSmartRules.flag(tags, "Clean"));
        tags.put("content_advisory", "Clean");
        assertTrue(PhoneSmartRules.flag(tags, "Clean"));
        tags.put("content_advisory", "clean");
        assertFalse(PhoneSmartRules.flag(tags, "Clean"));
        assertFalse(PhoneSmartRules.flag(new JSONObject("{\"title\":\"Clean radio edit\"}"), "Clean"));
    }
    @Test public void conflictingAndUntypedTagsNeverProveClean() throws Exception {
        for (String array : new String[]{"[]", "[2]", "[\"0\"]", "[\"2\",\"1\"]", "[\"2\",\"unrated\"]"}) {
            JSONObject p = new JSONObject("{\"embedded_tags\":{\"ParentalAdvisory\":" + array + "}}");
            assertFalse(array, ContentAdvisory.isKnownClean(p));
        }
        assertTrue(ContentAdvisory.isKnownClean(new JSONObject("{\"extensions\":{\"content_advisory\":\"Clean\"}}")));
        assertFalse(ContentAdvisory.isKnownClean(new JSONObject("{\"extensions\":{\"content_advisory\":\"Clean\"},\"content_advisory\":\"Unknown\"}")));
    }
}
