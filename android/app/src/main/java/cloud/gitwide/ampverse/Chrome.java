package cloud.gitwide.ampverse;

import android.graphics.Color;
import android.graphics.drawable.Drawable;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.LayerDrawable;
import android.graphics.drawable.StateListDrawable;

/** Beveled Classic chrome; the same views use flat surfaces in modern themes. */
final class Chrome {
    static Drawable panel(Appearance colors, boolean display) {
        if (!colors.classic) return gradient(colors.surface, colors.surface, colors.muted, 12);
        return bevel(display ? Color.rgb(5,7,9) : Color.rgb(51,52,61), display ? Color.rgb(12,14,18) : Color.rgb(31,32,39));
    }
    static Drawable button(Appearance colors, boolean selected) {
        if (!colors.classic) return gradient(selected ? colors.accent : colors.surface, selected ? colors.accent : colors.surface, colors.muted, 9);
        StateListDrawable states=new StateListDrawable();
        states.addState(new int[]{android.R.attr.state_pressed}, bevel(0xff827225,0xffd5ad22));
        states.addState(new int[]{}, selected ? bevel(0xffffee77,0xffb58c00) : bevel(0xffc8cbd1,0xff707480));
        return states;
    }
    private static Drawable bevel(int top,int bottom) {
        Drawable[] layers={gradient(0xffdce0e9,0xff181a24,0xff7b7f90,4),gradient(0xff1b1c27,0xffa6a9b6,0xff13141b,3),gradient(top,bottom,0xff555966,2)};
        LayerDrawable drawable=new LayerDrawable(layers);drawable.setLayerInset(1,2,2,2,2);drawable.setLayerInset(2,4,4,4,4);return drawable;
    }
    private static Drawable gradient(int top,int bottom,int edge,int radius) {
        GradientDrawable drawable=new GradientDrawable(GradientDrawable.Orientation.TOP_BOTTOM,new int[]{top,bottom});drawable.setCornerRadius(radius);drawable.setStroke(1,edge);return drawable;
    }
}
