package cloud.gitwide.ampverse;

/** A cancelled native job still owns its slot until both native completion and reader release. */
final class VinylDecodeGate {
    static final class Ticket {private boolean terminal,released,cleaned;}
    private Ticket active;
    synchronized Ticket acquire(){if(active!=null)return null;active=new Ticket();return active;}
    synchronized boolean terminal(Ticket ticket){ticket.terminal=true;return finish(ticket);}
    synchronized boolean release(Ticket ticket){ticket.released=true;return finish(ticket);}
    private boolean finish(Ticket ticket){
        if(!ticket.terminal||!ticket.released||ticket.cleaned)return false;
        ticket.cleaned=true;if(active==ticket)active=null;return true;
    }
}
