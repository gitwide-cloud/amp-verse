package cloud.gitwide.ampverse;

import org.json.JSONObject;
import org.junit.Test;
import java.util.Arrays;
import static org.junit.Assert.*;

public class SeasonalMusicTest {
    @Test public void holidayAndSeasonLabelsStayIndependent() throws Exception {
        JSONObject p=new JSONObject("{\"embedded_genres\":[\"Christmas\"],\"embedded_tags\":{\"AlbumTitle\":[\"Summer Hits\"],\"TrackArtist\":[\"Winter\"]}}");
        assertEquals(Arrays.asList("Christmas"),SeasonalMusic.values(p,"Holiday"));
        assertEquals(Arrays.asList("Summer"),SeasonalMusic.values(p,"Season"));
        assertTrue(SeasonalMusic.values(new JSONObject("{\"genre\":\"Christmas\"}"),"Season").isEmpty());
        assertTrue(SeasonalMusic.values(new JSONObject("{\"title\":\"Christmasland\",\"artist\":\"Christmas\"}"),"Holiday").isEmpty());
        assertTrue(SeasonalMusic.values(new JSONObject("{\"title\":\"Winter Wonderland\",\"album\":\"Fall Out Boy\"}"),"Season").isEmpty());
    }
    @Test public void explicitAliasesAndLegacyPreparationMatchDesktop() throws Exception {
        JSONObject p=new JSONObject("{\"extensions\":{\"holiday\":[\"Xmas\",\"New Year's Eve\"],\"season\":\"Fall\"}}");
        assertEquals(Arrays.asList("Christmas","New Year"),SeasonalMusic.values(p,"Holiday"));
        assertEquals(Arrays.asList("Autumn"),SeasonalMusic.values(p,"Season"));
        assertEquals("Text",PhoneSmartRules.kind("Holiday"));assertEquals("Text",PhoneSmartRules.kind("Season"));
        assertEquals(PhoneSmartRules.FIELDS.length,PhoneSmartRules.LABELS.length);
    }
}
