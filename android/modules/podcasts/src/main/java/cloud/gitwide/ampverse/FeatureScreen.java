package cloud.gitwide.ampverse;
final class FeatureScreen {
 final CompanionActivity a;PodcastsPanel panel;
 FeatureScreen(CompanionActivity a){this.a=a;}
 void open(android.widget.LinearLayout parent){panel=new PodcastsPanel(a,a.colors);parent.addView(panel);}
 void close(){if(panel!=null)panel.close();}
}
