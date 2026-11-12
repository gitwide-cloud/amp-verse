package cloud.gitwide.ampverse;

import android.content.Context;
import android.graphics.*;
import android.graphics.drawable.Drawable;

/** Small scalable line icons; labels remain visible and accessible. */
final class NavIcon extends Drawable {
    private final String name;
    private final Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG);
    private final int size;
    NavIcon(Context context,String name,int color){this.name=name;size=(int)(20*context.getResources().getDisplayMetrics().density);paint.setColor(color);paint.setStrokeWidth(1.8f);paint.setStrokeCap(Paint.Cap.ROUND);}
    @Override public void draw(Canvas canvas){canvas.save();canvas.translate(getBounds().left,getBounds().top);canvas.scale(getBounds().width()/24f,getBounds().height()/24f);paint.setStyle(Paint.Style.STROKE);
        if(name.equals("Player")){Path p=new Path();p.moveTo(7,4);p.lineTo(20,12);p.lineTo(7,20);p.close();canvas.drawPath(p,paint);}
        else if(name.equals("Library")){for(int y=5;y<=19;y+=7){canvas.drawCircle(4,y,1,paint);canvas.drawLine(9,y,21,y,paint);}}
        else if(name.equals("DJ")){canvas.drawCircle(7,12,5,paint);canvas.drawCircle(17,12,5,paint);canvas.drawCircle(7,12,1,paint);canvas.drawCircle(17,12,1,paint);}
        else{canvas.drawRoundRect(3,3,21,17,2,2,paint);canvas.drawLine(12,17,12,21,paint);canvas.drawLine(8,21,16,21,paint);}canvas.restore();}
    @Override public int getIntrinsicWidth(){return size;}@Override public int getIntrinsicHeight(){return size;}
    @Override public void setAlpha(int alpha){paint.setAlpha(alpha);invalidateSelf();}@Override public void setColorFilter(ColorFilter filter){paint.setColorFilter(filter);invalidateSelf();}
    @Override public int getOpacity(){return PixelFormat.TRANSLUCENT;}
}
