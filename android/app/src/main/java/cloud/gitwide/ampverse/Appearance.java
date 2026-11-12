package cloud.gitwide.ampverse;

import android.content.Context;
import android.graphics.Color;

/** One application palette shared by Player, Library and Settings. */
public final class Appearance {
    public final int background, surface, text, muted, accent;
    public final boolean classic;
    public Appearance(Context context) {
        String theme = context.getSharedPreferences("settings", Context.MODE_PRIVATE).getString("theme", "classic");
        boolean light = theme.equals("modern-light"); classic = theme.equals("classic");
        background = Color.parseColor(light ? "#f1f3f9" : classic ? "#191c20" : "#111418");
        surface = Color.parseColor(light ? "#ffffff" : classic ? "#292e34" : "#20262d");
        text = Color.parseColor(light ? "#202332" : "#f0eef7");
        muted = Color.parseColor(light ? "#647089" : "#a6aeb9");
        accent = Color.parseColor(light ? "#b87500" : classic ? "#ffca38" : "#ffc247");
    }
}
