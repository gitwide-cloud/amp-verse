package cloud.gitwide.ampverse;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;

/** Identity and retry policy only: neither playback nor credentials belong in durable selection. */
final class CastHostSelection {
    static final class Host {
        final String base, server, station;
        Host(String base,String server,String station){this.base=base;this.server=server;this.station=station;}
        String identity(){return server+"/"+station;}
        boolean matches(String server,String station){return this.server.equals(server)&&this.station.equals(station);}
    }
    private Host selected;
    private long generation;
    private int failures;
    private long retryAt;
    private String network="";
    private final LinkedHashMap<String,CastDiscovery.Server> discovered=new LinkedHashMap<>();
    CastHostSelection(Host restored){selected=valid(restored)?restored:null;}
    private static boolean valid(Host host){return host!=null&&!host.base.isEmpty()&&!host.server.isEmpty()&&!host.station.isEmpty();}
    synchronized Host selected(){return selected;}
    synchronized long generation(){return generation;}
    synchronized boolean current(long token){return generation==token;}
    synchronized void select(Host host){if(!valid(host))throw new IllegalArgumentException("Verified host identity required");selected=host;generation++;recovered();}
    synchronized void disconnect(){selected=null;generation++;recovered();}
    synchronized void recovered(){failures=0;retryAt=0;}
    synchronized void failed(long now){failures=Math.min(7,failures+1);retryAt=now+Math.min(60000L,2000L<<(failures-1));}
    synchronized boolean retryDue(long now){return selected!=null&&now>=retryAt;}
    synchronized boolean discoveryDue(){return failures==2||failures==4||failures==6;}
    synchronized boolean networkChanged(String signature){if(network.equals(signature))return false;network=signature;recovered();return true;}
    synchronized List<CastDiscovery.Server> merge(List<CastDiscovery.Server> fresh){
        for(CastDiscovery.Server server:fresh){if(!discovered.containsKey(server.id)&&discovered.size()>=16)continue;discovered.put(server.id,server);}
        return new ArrayList<>(discovered.values());
    }
}
