package cloud.gitwide.ampverse;

import android.graphics.Color;
import android.graphics.drawable.Drawable;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.StateListDrawable;

/** Beveled Classic chrome; the same views use flat surfaces in modern themes. */
final class Chrome {
    private static int dp(int n){return Math.round(n*android.content.res.Resources.getSystem().getDisplayMetrics().density);}
    static Drawable panel(Appearance colors, boolean display) {
        boolean light=Color.red(colors.background)>200;
        int top=display?colors.background:colors.surface;
        int bottom=light?top:display?0xff101519:0xff171c21;
        return gradient(top,bottom,light?0xffc9ced6:0xff59616a,dp(14));
    }
    static Drawable highlighted(Appearance colors,int accent){return gradient(colors.surface,Color.red(colors.background)>200?colors.surface:0xff141a1e,accent,dp(14));}
    static int ink(Appearance colors,boolean selected){return selected ? Color.BLACK : colors.text;}
    static Drawable button(Appearance colors, boolean selected) {
        StateListDrawable states=new StateListDrawable();
        states.addState(new int[]{-android.R.attr.state_enabled},gradient(colors.surface,colors.surface,0xff505862,dp(10)));
        states.addState(new int[]{android.R.attr.state_pressed},gradient(selected?0xffffe267:colors.surface,selected?colors.accent:colors.background,colors.accent,dp(10)));
        states.addState(new int[]{},gradient(selected?0xffffdc48:colors.surface,selected?0xffffc71c:Color.red(colors.background)>200?colors.surface:0xff171d22,selected?colors.accent:0xff68717a,dp(10)));
        return states;
    }
    static void artwork(android.widget.ImageView image,androidx.media3.common.MediaItem item,int accent){
        artworkBytes(image,item==null?null:item.mediaMetadata.artworkData,accent);
    }
    static void artworkBytes(android.widget.ImageView image,byte[] bytes,int accent){
        image.setImageDrawable(new CardIcon(image.getContext(),"vinyl",accent));
        image.setScaleType(android.widget.ImageView.ScaleType.CENTER_INSIDE);
        image.setBackground(highlighted(new Appearance(image.getContext()),accent));
        image.setClipToOutline(true);
        if(bytes==null)return;
        android.graphics.BitmapFactory.Options options=new android.graphics.BitmapFactory.Options();options.inJustDecodeBounds=true;
        android.graphics.BitmapFactory.decodeByteArray(bytes,0,bytes.length,options);
        options.inSampleSize=Math.max(1,Math.max(options.outWidth,options.outHeight)/384);options.inJustDecodeBounds=false;
        android.graphics.Bitmap bitmap=android.graphics.BitmapFactory.decodeByteArray(bytes,0,bytes.length,options);
        if(bitmap!=null){image.setImageBitmap(bitmap);image.setScaleType(android.widget.ImageView.ScaleType.CENTER_CROP);}
    }
    static void slider(android.widget.SeekBar slider,int color){android.content.res.ColorStateList tint=android.content.res.ColorStateList.valueOf(color);slider.setProgressTintList(tint);slider.setThumbTintList(tint);}
    static void toggle(android.widget.Switch toggle,Appearance colors){toggle.setThumbTintList(new android.content.res.ColorStateList(new int[][]{new int[]{android.R.attr.state_checked},new int[]{}},new int[]{colors.accent,colors.muted}));}
    private static Drawable gradient(int top,int bottom,int edge,int radius) {
        GradientDrawable drawable=new GradientDrawable(GradientDrawable.Orientation.TOP_BOTTOM,new int[]{top,bottom});drawable.setCornerRadius(radius);drawable.setStroke(1,edge);return drawable;
    }
}
