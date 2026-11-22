package cloud.gitwide.ampverse;

import java.io.IOException;
import org.json.JSONObject;

/** Read-only reattachment. Identity is checked publicly before any memory-only credential is reused. */
final class CastHostReconnect {
    interface PublicClient {CastClient create(String base)throws Exception;}
    static final class Verified {
        final CastClient client;final JSONObject status;
        Verified(CastClient client,JSONObject status){this.client=client;this.status=status;}
    }
    static void verify(JSONObject status,String identity)throws IOException {
        if(identity!=null&&!identity.equals(status.optString("server_id")+"/"+status.optString("station_id")))
            throw new IOException("Selected host identity changed. Search and explicitly select the intended server; no playback command was sent.");
    }
    static Verified restore(String base,String identity,CastClient memory)throws Exception {
        return restore(base,identity,memory,address->new CastClient(address,""));
    }
    static Verified restore(String base,String identity,CastClient memory,PublicClient factory)throws Exception {
        CastClient publicClient=factory.create(base);JSONObject publicStatus=publicClient.connect();
        verify(publicStatus,identity);
        CastClient selected=memory!=null&&memory.base.equals(base)?memory:publicClient;
        JSONObject status=selected==publicClient?publicStatus:selected.status();verify(status,identity);
        return new Verified(selected,status);
    }
}
