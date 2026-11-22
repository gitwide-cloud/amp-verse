package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;
public final class VinylDecodeGateTest {
    @Test public void cancelledCallerCannotStartSecondNativeJob(){VinylDecodeGate gate=new VinylDecodeGate();VinylDecodeGate.Ticket first=gate.acquire();assertFalse(gate.release(first));assertNull(gate.acquire());assertTrue(gate.terminal(first));assertNotNull(gate.acquire());}
    @Test public void completionCannotDeleteWindowWhileReaderOwnsIt(){VinylDecodeGate gate=new VinylDecodeGate();VinylDecodeGate.Ticket first=gate.acquire();assertFalse(gate.terminal(first));assertNull(gate.acquire());assertTrue(gate.release(first));assertNotNull(gate.acquire());}
    @Test public void lateOldCallbackCannotUnlockNewJobOrCleanTwice(){VinylDecodeGate gate=new VinylDecodeGate();VinylDecodeGate.Ticket first=gate.acquire();gate.terminal(first);assertTrue(gate.release(first));VinylDecodeGate.Ticket next=gate.acquire();assertFalse(gate.terminal(first));assertFalse(gate.release(first));assertNull(gate.acquire());assertFalse(gate.release(next));assertTrue(gate.terminal(next));assertNotNull(gate.acquire());}
}
