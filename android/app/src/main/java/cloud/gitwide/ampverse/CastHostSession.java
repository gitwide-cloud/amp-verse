package cloud.gitwide.ampverse;

import android.content.Context;
import android.content.SharedPreferences;

/** Application-scoped host selection survives view/activity replacement; no Activity or secret is persisted. */
final class CastHostSession {
    private static CastHostSession instance;
    static synchronized CastHostSession get(Context context){
        if(instance==null)instance=new CastHostSession(context.getApplicationContext());
        return instance;
    }
    final CastHostSelection selection;
    private final SharedPreferences settings;
    private CastClient authenticated;
    private CastHostSession(Context app){
        settings=app.getSharedPreferences("settings",Context.MODE_PRIVATE);
        selection=new CastHostSelection(new CastHostSelection.Host(settings.getString("cast-server",""),settings.getString("cast-server-id",""),settings.getString("cast-station-id","")));
    }
    synchronized CastClient client(){return authenticated;}
    synchronized void remember(CastClient client,String server,String station){
        selection.select(new CastHostSelection.Host(client.base,server,station));authenticated=client;
        settings.edit().putString("cast-server",client.base).putString("cast-server-id",server).putString("cast-station-id",station).apply();
    }
    synchronized void disconnect(){
        selection.disconnect();authenticated=null;
        settings.edit().remove("cast-server").remove("cast-server-id").remove("cast-station-id").apply();
    }
}
