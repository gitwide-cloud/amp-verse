package cloud.gitwide.ampverse;
import android.content.Context;
import android.graphics.*;
import android.graphics.drawable.Drawable;
/** Destination symbols, deliberately static: these are never audio level meters. */
final class DevicesIcon extends Drawable {
    private final Paint p=new Paint(Paint.ANTI_ALIAS_FLAG);private final String kind;private final int size;
    DevicesIcon(Context context,String kind,int color){this.kind=kind;size=Math.round(24*context.getResources().getDisplayMetrics().density);p.setColor(color);p.setStrokeWidth(1.8f);p.setStrokeCap(Paint.Cap.ROUND);p.setStrokeJoin(Paint.Join.ROUND);}
    @Override public void draw(Canvas c){c.save();c.translate(getBounds().left,getBounds().top);c.scale(getBounds().width()/24f,getBounds().height()/24f);p.setStyle(Paint.Style.STROKE);
        if(kind.equals("speaker")){Path path=new Path();path.moveTo(3,9);path.lineTo(7,9);path.lineTo(12,4);path.lineTo(12,20);path.lineTo(7,15);path.lineTo(3,15);path.close();c.drawPath(path,p);c.drawArc(10,5,21,19,-65,130,false,p);}
        else if(kind.equals("radio")){c.drawRoundRect(3,7,21,21,3,3,p);c.drawLine(5,7,18,2,p);c.drawCircle(8,14,2,p);c.drawLine(14,12,18,12,p);c.drawLine(14,16,18,16,p);}
        else if(kind.equals("globe")){c.drawCircle(12,12,10,p);c.drawOval(7,2,17,22,p);c.drawLine(2,12,22,12,p);}
        else if(kind.equals("remote")){c.drawRoundRect(7,3,17,22,3,3,p);c.drawCircle(12,10,2,p);c.drawPoint(10,17,p);c.drawPoint(14,17,p);c.drawArc(3,0,21,12,210,120,false,p);}
        else {c.drawCircle(12,12,10,p);Path path=new Path();path.moveTo(16,7);path.lineTo(9,10);path.lineTo(7,17);path.lineTo(14,14);path.close();c.drawPath(path,p);}c.restore();}
    @Override public int getIntrinsicWidth(){return size;}@Override public int getIntrinsicHeight(){return size;}
    @Override public void setAlpha(int alpha){p.setAlpha(alpha);}@Override public void setColorFilter(ColorFilter filter){p.setColorFilter(filter);}@Override public int getOpacity(){return PixelFormat.TRANSLUCENT;}
}
