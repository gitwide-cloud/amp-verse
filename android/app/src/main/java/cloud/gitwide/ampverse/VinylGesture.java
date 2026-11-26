package cloud.gitwide.ampverse;

/** Angular hand travel: one revolution is 1.8 seconds (33 1/3 RPM), with seam-safe reversal. */
final class VinylGesture {
    private double angle;private boolean held,tracking;
    static boolean hit(double x,double y,double radius,double margin){return radius>0&&Math.hypot(x,y)<=radius+Math.max(0,margin);}
    void downPixels(double x,double y,double radius){down(x/Math.max(1,radius),y/Math.max(1,radius));}
    double movePixels(double x,double y,double radius){return move(x/Math.max(1,radius),y/Math.max(1,radius));}
    void down(double x,double y){angle=Math.atan2(y,x);held=true;tracking=Math.hypot(x,y)>=.15;}
    double move(double x,double y){if(!held)return 0;if(Math.hypot(x,y)<.15){tracking=false;return 0;}double next=Math.atan2(y,x);if(!tracking){angle=next;tracking=true;return 0;}double delta=next-angle;angle=next;while(delta>Math.PI)delta-=Math.PI*2;while(delta< -Math.PI)delta+=Math.PI*2;return delta/(2*Math.PI)*1.8;}
    void up(){held=false;}
}
