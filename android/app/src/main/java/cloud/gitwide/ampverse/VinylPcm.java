package cloud.gitwide.ampverse;

/** Bounded stereo source PCM, resampled in either direction to follow the hand. No Android APIs. */
final class VinylPcm {
    final short[] samples;final int rate;final long startMs;final int frames;
    private final double initial;private double cursor,target;private int remaining;private long gestureNanos;
    private long rendered;private final double[] heard=new double[32768];
    VinylPcm(short[] samples,int rate,long startMs,long positionMs){
        if(rate<=0||samples.length<4||samples.length%2!=0||samples.length>rate*2*8)throw new IllegalArgumentException("Scratch window must contain at most eight seconds of stereo PCM");
        this.samples=samples;this.rate=rate;this.startMs=startMs;frames=samples.length/2;cursor=target=clamp((positionMs-startMs)*rate/1000.);initial=cursor;java.util.Arrays.fill(heard,cursor);
    }
    private double clamp(double position){return Math.max(0,Math.min(frames-1,position));}
    synchronized void hold(long now){gestureNanos=now;target=cursor;remaining=0;}
    synchronized void drag(double deltaFrames,long now){
        if(!Double.isFinite(deltaFrames))return;
        double seconds=gestureNanos==0?.016:(now-gestureNanos)/1_000_000_000.;gestureNanos=now;
        target=clamp(target+deltaFrames);remaining=(int)Math.max(64,Math.min(rate/20.,seconds*rate));
    }
    synchronized void render(short[] output,int count){
        if(count*2>output.length)throw new IllegalArgumentException("Short scratch output buffer");
        for(int i=0;i<count;i++){
            double distance=target-cursor;
            if(Math.abs(distance)<.001||remaining==0){output[i*2]=output[i*2+1]=0;remaining=0;}
            else {
                double step=Math.max(-8,Math.min(8,distance/remaining));
                int a=(int)cursor,b=Math.min(a+1,frames-1);double part=cursor-a;
                for(int channel=0;channel<2;channel++)output[i*2+channel]=(short)Math.round(samples[a*2+channel]*(1-part)+samples[b*2+channel]*part);
                cursor=clamp(cursor+step);remaining--;
                if(remaining==0&&Math.abs(target-cursor)>.001)remaining=1;
            }
            heard[(int)(rendered%heard.length)]=cursor;rendered++;
        }
    }
    synchronized long positionMs(){return startMs+Math.round(cursor*1000/rate);}
    synchronized long heardPositionMs(long playedFrames){
        long frame=Math.max(0,Math.min(rendered,playedFrames));
        double position=frame==0?initial:heard[(int)((frame-1)%heard.length)];
        return startMs+Math.round(position*1000/rate);
    }
    boolean covers(long positionMs,long marginMs){long duration=frames*1000L/rate;return positionMs>=startMs+(startMs==0?0:marginMs)&&positionMs<=startMs+duration-(duration<7990?0:marginMs);}
}
