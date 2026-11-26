package cloud.gitwide.ampverse;
import org.junit.Test;
import static org.junit.Assert.*;

public class ReleaseChannelTest {
    @Test public void betaVersionOrderingKeepsStableUpgradeAndSafeModuleCatalogTags(){
        assertEquals(0,ReleaseCheck.compare("v0.3.2-beta.1","0.3.2-beta.1"));
        assertTrue(ReleaseCheck.compare("0.3.2-beta.2","0.3.2-beta.1")>0);
        assertTrue(ReleaseCheck.compare("0.3.2","0.3.2-beta.9")>0);
        assertTrue(ReleaseCheck.compare("0.3.2-beta.1","0.3.1")>0);
        assertTrue(ReleaseCheck.betaVersion("v0.3.2-beta.1"));
        for(String bad:new String[]{"0.3.2-preview.1","0.3.2-beta.0","0.3.2-beta.01","0.3.2-beta.1/other","0.3.2-beta.1+other","0.3.2"})
            assertFalse(bad,ReleaseCheck.betaVersion(bad));
        for(String bad:new String[]{"0.3.2-preview.1","0.3.2-beta.0","0.3.2-beta.01","0.3.2-beta.1/other"}){
            try{ReleaseCheck.compare(bad,"0.3.2");fail(bad);}catch(IllegalArgumentException expected){}
        }
    }
    @Test public void releaseCandidateCanPromoteToStableWithoutBreakingTheUpdater(){
        assertTrue(ReleaseCheck.compare("1.1.0","1.1.0-rc.1")>0);
        assertTrue(ReleaseCheck.compare("1.1.0-rc.2","1.1.0-rc.1")>0);
        assertTrue(ReleaseCheck.compare("1.1.0-rc.1","1.1.0-beta.9")>0);
        assertEquals(0,ReleaseCheck.compare("v1.1.0-rc.1","1.1.0-rc.1"));
    }

    @Test public void releaseCandidateModuleCatalogStaysInItsOwnChannel(){
        assertTrue(ReleaseCheck.samePrereleaseChannel("1.1.0-rc.1","v1.1.0-rc.2"));
        assertTrue(ReleaseCheck.samePrereleaseChannel("1.1.0-beta.1","1.1.0-beta.2"));
        assertFalse(ReleaseCheck.samePrereleaseChannel("1.1.0-rc.1","1.1.0-beta.2"));
        assertFalse(ReleaseCheck.samePrereleaseChannel("1.1.0","1.1.0-rc.1"));
        for(String bad:new String[]{"1.1.0-rc.0","1.1.0-rc.01","1.1.0-rc.1/other","1.1.0-preview.1"}){
            assertFalse(ReleaseCheck.rcVersion(bad));try{ReleaseCheck.compare(bad,"1.1.0");fail();}catch(IllegalArgumentException expected){}
        }
    }

}
