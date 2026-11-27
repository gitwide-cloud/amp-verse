package cloud.gitwide.ampverse;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotEquals;

public class PhoneSmartRulesGenreTest {
    @Test public void normalizedTagsKeepLegacyGenreFiltersMatching() {
        String[][] aliases={{"Hip Hop","Hip-Hop"},{"hiphop","Hip-Hop"},{"DNB","Drum & Bass"},{"drum and bass","Drum & Bass"},{"Acapella","A cappella"},{"RNB","R&B"}};
        for(String[] pair:aliases) assertEquals(PhoneSmartRules.genre(pair[0]),PhoneSmartRules.genre(pair[1]));
        assertNotEquals(PhoneSmartRules.genre("Rock"),PhoneSmartRules.genre("Pop"));
    }
}
