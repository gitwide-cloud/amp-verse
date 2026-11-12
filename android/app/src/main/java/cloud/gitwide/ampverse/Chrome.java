package cloud.gitwide.ampverse;

import android.graphics.Color;
import android.graphics.drawable.Drawable;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.StateListDrawable;

/** Beveled Classic chrome; the same views use flat surfaces in modern themes. */
final class Chrome {
    static Drawable panel(Appearance colors, boolean display) {
        if (!colors.classic) return gradient(display ? colors.background : colors.surface, display ? colors.background : colors.surface, Color.argb(45,128,138,150), 12);
        return gradient(display ? 0xff111416 : 0xff2a2e33, display ? 0xff111416 : 0xff22262b, 0xff454a51, 5);
    }
    static int ink(Appearance colors,boolean selected){return selected ? Color.BLACK : colors.text;}
    static Drawable button(Appearance colors, boolean selected) {
        StateListDrawable states=new StateListDrawable();
        states.addState(new int[]{android.R.attr.state_pressed}, gradient(selected?colors.accent:colors.surface,selected?colors.accent:colors.surface,colors.accent,8));
        states.addState(new int[]{}, gradient(selected?colors.accent:colors.classic?0xff424852:colors.surface,selected?(colors.classic?0xffefa92b:colors.accent):colors.classic?0xff292e36:colors.surface,selected?colors.accent:Color.argb(100,128,138,150),8));
        return states;
    }
    static void slider(android.widget.SeekBar slider,int color){android.content.res.ColorStateList tint=android.content.res.ColorStateList.valueOf(color);slider.setProgressTintList(tint);slider.setThumbTintList(tint);}
    static void toggle(android.widget.Switch toggle,Appearance colors){toggle.setThumbTintList(new android.content.res.ColorStateList(new int[][]{new int[]{android.R.attr.state_checked},new int[]{}},new int[]{colors.accent,colors.muted}));}
    private static Drawable gradient(int top,int bottom,int edge,int radius) {
        GradientDrawable drawable=new GradientDrawable(GradientDrawable.Orientation.TOP_BOTTOM,new int[]{top,bottom});drawable.setCornerRadius(radius);drawable.setStroke(1,edge);return drawable;
    }
}
