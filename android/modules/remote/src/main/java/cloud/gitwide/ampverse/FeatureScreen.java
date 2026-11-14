package cloud.gitwide.ampverse;
final class FeatureScreen {
 private final CompanionActivity a;private RemotePanel panel;
 FeatureScreen(CompanionActivity a){this.a=a;}
 void open(android.widget.LinearLayout parent){panel=new RemotePanel(a,a.colors);parent.addView(panel);}
 void close(){if(panel!=null)panel.close();}
}
