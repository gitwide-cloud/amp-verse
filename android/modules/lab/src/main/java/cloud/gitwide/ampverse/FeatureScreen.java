package cloud.gitwide.ampverse;
final class FeatureScreen {
 private final CompanionActivity a;
 FeatureScreen(CompanionActivity a){this.a=a;}
 void open(android.widget.LinearLayout parent){a.startActivity(new android.content.Intent(a,cloud.gitwide.ampverse.lab.MainActivity.class).putExtra("amp-test-keep-awake",a.getIntent().getBooleanExtra("amp-test-keep-awake",false)));a.finish();}
 void close(){}
}
