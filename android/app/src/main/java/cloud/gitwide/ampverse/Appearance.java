package cloud.gitwide.ampverse;

import android.content.Context;
import android.graphics.Color;

/** One application palette shared by Player, Library and Settings. */
final class Appearance {
    final int background, surface, text, muted, accent;
    final boolean classic;
    Appearance(Context context) {
        String theme = context.getSharedPreferences("settings", Context.MODE_PRIVATE).getString("theme", "classic");
        boolean light = theme.equals("modern-light"); classic = theme.equals("classic");
        background = Color.parseColor(light ? "#f1f3f9" : classic ? "#222426" : "#10141d");
        surface = Color.parseColor(light ? "#ffffff" : classic ? "#55585e" : "#202735");
        text = Color.parseColor(light ? "#202332" : "#f0eef7");
        muted = Color.parseColor(light ? "#647089" : "#aaa4bd");
        accent = Color.parseColor(light ? "#7142cc" : classic ? "#ffd52b" : "#b586ff");
    }
}
