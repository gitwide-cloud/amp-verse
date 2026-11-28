package cloud.gitwide.ampverse;

import org.json.JSONArray;
import org.json.JSONObject;

/** Same conservative Clean decision as amp-core; titles never establish it. */
final class ContentAdvisory {
    private ContentAdvisory() {}

    static boolean isKnownClean(JSONObject preparation) {
        JSONObject values = PhoneSmartRules.extensions(preparation);
        // An explicit Unknown (or malformed override) must not fall back to tags.
        if (values.has("content_advisory")) {
            return "Clean".equals(values.opt("content_advisory"));
        }
        JSONArray tags = PhoneSmartRules.object(values, "embedded_tags")
                .optJSONArray("ParentalAdvisory");
        if (tags == null || tags.length() == 0) return false;
        for (int i = 0; i < tags.length(); i++) {
            Object tag = tags.opt(i);
            if (!(tag instanceof String)) return false;
            String value = ((String) tag).trim();
            if (!(value.equals("2") || value.equals("Clean") || value.equals("clean"))) {
                return false;
            }
        }
        return true;
    }
}
