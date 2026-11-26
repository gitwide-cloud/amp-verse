package cloud.gitwide.ampverse;
final class DjPanel extends android.widget.LinearLayout {
 interface Host {void effect(int side,int pad);void beforeMainPlay();void equalizer();}
 final DjEngine engine;
 DjPanel(android.app.Activity a,Appearance c,DjEngine e,Host h){super(a);engine=e;}
 android.view.View compactMixer(){return null;} void redraw(){}
}
