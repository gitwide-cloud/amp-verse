package cloud.gitwide.ampverse;
/** Runs as the test provider owner, as a real file manager would. */
public final class ExternalAudioSenderActivity extends android.app.Activity {
    @Override public void onCreate(android.os.Bundle state){
        super.onCreate(state);
        String path=getIntent().getBooleanExtra("missing",false)?"missing":"song";
        android.net.Uri uri=android.net.Uri.parse("content://cloud.gitwide.ampverse.test.external/"+path);
        android.content.Intent open=new android.content.Intent(android.content.Intent.ACTION_VIEW)
            .setDataAndType(uri,"audio/wav").setPackage("cloud.gitwide.ampverse")
            .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK|android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION);
        open.setClipData(android.content.ClipData.newRawUri("Audio",uri));
        startActivity(open);finish();
    }
}
