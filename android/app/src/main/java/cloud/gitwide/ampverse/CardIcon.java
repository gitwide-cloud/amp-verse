package cloud.gitwide.ampverse;

import android.content.Context;
import android.graphics.*;
import android.graphics.drawable.Drawable;

/** Static vector identity and honest artwork fallback; never an invented audio meter. */
final class CardIcon extends Drawable {
    private final Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG);
    private final String kind;private final int size,color;
    CardIcon(Context context,String kind,int color){this.kind=kind;this.color=color;size=Math.round(context.getResources().getDisplayMetrics().density*(kind.equals("spectrum")?58:32));}
    @Override public void draw(Canvas c){c.save();c.translate(getBounds().left,getBounds().top);c.scale(getBounds().width()/32f,getBounds().height()/32f);paint.setColor(color);paint.setStrokeWidth(1.7f);paint.setStrokeJoin(Paint.Join.ROUND);paint.setStrokeCap(Paint.Cap.ROUND);paint.setStyle(Paint.Style.STROKE);
        if(kind.equals("spectrum")){paint.setStyle(Paint.Style.FILL);int[] heights={1,2,4,6,4,8,12,16,13,10,7,5,3,2};for(int i=0;i<heights.length;i++)for(int j=0;j<heights[i];j+=2)c.drawRect(i*2.2f,25-j,i*2.2f+1.6f,26-j,paint);}
        else if(kind.equals("folder")){c.drawRoundRect(3,9,29,27,3,3,paint);c.drawLine(4,9,4,5,paint);c.drawLine(4,5,13,5,paint);c.drawLine(13,5,17,9,paint);c.drawCircle(14,22,2,paint);c.drawLine(16,22,16,14,paint);c.drawLine(16,14,23,12,paint);}
        else if(kind.equals("server")){for(int y=4;y<27;y+=9){c.drawRoundRect(3,y,29,y+6,2,2,paint);c.drawCircle(7,y+3,.7f,paint);c.drawLine(13,y+3,24,y+3,paint);}}
        else if(kind.equals("playlist")){for(int y=7;y<29;y+=9){c.drawCircle(4,y,1,paint);c.drawLine(10,y,28,y,paint);}}
        else if(kind.equals("download")){c.drawLine(16,3,16,23,paint);c.drawLine(9,16,16,23,paint);c.drawLine(16,23,23,16,paint);c.drawLine(4,23,4,29,paint);c.drawLine(4,29,28,29,paint);c.drawLine(28,29,28,23,paint);}
        else{c.drawCircle(16,16,14,paint);paint.setColor(0xff64707c);for(int ring=6;ring<14;ring+=2)c.drawCircle(16,16,ring,paint);paint.setColor(color);Path bolt=new Path();bolt.moveTo(18,6);bolt.lineTo(11,18);bolt.lineTo(16,18);bolt.lineTo(14,26);bolt.lineTo(22,13);bolt.lineTo(17,13);bolt.close();paint.setStyle(Paint.Style.FILL);c.drawPath(bolt,paint);}c.restore();
    }
    @Override public int getIntrinsicWidth(){return size;}@Override public int getIntrinsicHeight(){return kind.equals("spectrum")?size/2:size;}
    @Override public void setAlpha(int a){paint.setAlpha(a);}@Override public void setColorFilter(ColorFilter f){paint.setColorFilter(f);}@Override public int getOpacity(){return PixelFormat.TRANSLUCENT;}
}
