package cloud.gitwide.ampverse;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.os.SystemClock;
import android.view.View;

@androidx.annotation.OptIn(markerClass = androidx.media3.common.util.UnstableApi.class)
final class SpectrumView extends View {
    private final Paint paint=new Paint();
    private final float[] levels=new float[24];
    SpectrumView(Context context){super(context);setContentDescription("Live audio spectrum");setBackgroundColor(Color.rgb(4,6,8));}
    @Override protected void onWindowVisibilityChanged(int visibility){super.onWindowVisibilityChanged(visibility);AudioDsp.visible=visibility==VISIBLE;}
    @Override protected void onDetachedFromWindow(){AudioDsp.visible=false;super.onDetachedFromWindow();}
    @Override protected void onDraw(Canvas canvas){
        super.onDraw(canvas);float[] current=AudioDsp.spectrum;boolean fresh=SystemClock.elapsedRealtime()-AudioDsp.spectrumTime<250;float width=getWidth()/24f,height=getHeight()/12f;
        for(int band=0;band<24;band++){
            float target=fresh?current[band]:0;levels[band]=Math.max(target,levels[band]-.055f);
            for(int cell=0;cell<12;cell++){paint.setColor(cell<levels[band]*12?Color.rgb(cell<6?85:220,cell<10?235:190,15):Color.rgb(20,24,26));canvas.drawRect(band*width+2,getHeight()-(cell+1)*height+1,(band+1)*width-2,getHeight()-cell*height-1,paint);}
        }
        if(getWindowVisibility()==VISIBLE)postInvalidateDelayed(33);
    }
}
