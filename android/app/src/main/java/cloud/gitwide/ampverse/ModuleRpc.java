package cloud.gitwide.ampverse;
import android.content.Context;
import android.net.Uri;
import android.os.Bundle;
import org.json.JSONObject;

/** Contract v1. Call from a worker; responses are bounded snapshots, never file paths. */
public final class ModuleRpc {
    private ModuleRpc(){}
    public static JSONObject call(Context context,String action,JSONObject value)throws Exception{
        Bundle args=new Bundle();args.putString("json",value==null?"{}":value.toString());
        Bundle result=context.getContentResolver().call(Uri.parse("content://cloud.gitwide.ampverse.modules"),"v1",action,args);
        if(result==null)throw new IllegalStateException("AMP Player is not connected");
        if(result.containsKey("error"))throw new IllegalStateException(result.getString("error"));
        return new JSONObject(result.getString("json","{}"));
    }
}
