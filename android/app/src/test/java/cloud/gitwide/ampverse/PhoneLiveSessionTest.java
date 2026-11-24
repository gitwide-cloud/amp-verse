package cloud.gitwide.ampverse;

import org.junit.Test;
import static org.junit.Assert.*;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

public final class PhoneLiveSessionTest {
    private static PhoneLiveSession.Identity identity(String server){return new PhoneLiveSession.Identity(server,"station","library");}
    private static class Fake implements PhoneLiveSession.Transport {
        PhoneLiveSession.Identity current=identity("selected");
        int verified,starts,stops,uploads;
        long acknowledged;
        boolean loseFirstAck,failUploads,reject,legacy,changeAfterStart,loseStart,block;
        String previous,stoppedToken;
        final List<byte[]> packets=new ArrayList<>();
        final List<Long> sequences=new ArrayList<>();
        final CountDownLatch entered=new CountDownLatch(1),aborted=new CountDownLatch(1);
        @Override public PhoneLiveSession.Identity verify(){verified++;return changeAfterStart&&starts>0?identity("different"):current;}
        @Override public PhoneLiveSession.Started start(String previous)throws IOException {
            starts++;this.previous=previous;if(loseStart)throw new IOException("Unknown start acknowledgement");
            acknowledged=0;return new PhoneLiveSession.Started("owner-"+starts,0,!legacy,!legacy);
        }
        @Override public long upload(String token,long sequence,byte[] bytes)throws IOException {
            uploads++;packets.add(bytes.clone());sequences.add(sequence);
            if(reject)throw new PhoneLiveSession.Rejected("Owner was replaced");
            if(block){entered.countDown();try{aborted.await(5,TimeUnit.SECONDS);}catch(InterruptedException e){Thread.currentThread().interrupt();}throw new IOException("Aborted request");}
            if(failUploads)throw new IOException("Temporary network failure");
            if(sequence==acknowledged)acknowledged++;
            if(loseFirstAck&&uploads==1)throw new IOException("Acknowledgement lost after acceptance");
            return acknowledged;
        }
        @Override public void stop(String token){stops++;stoppedToken=token;}
        @Override public void abort(){aborted.countDown();}
    }
    private static PhoneLiveSession session(Fake transport){return new PhoneLiveSession(identity("selected"),transport);}

    @Test public void lostAudioAcknowledgementRetriesIdenticalOwnedPacket()throws Exception {
        Fake transport=new Fake();transport.loseFirstAck=true;PhoneLiveSession sender=session(transport);sender.begin();
        byte[] encoded={1,2,3,4};sender.send(encoded,encoded.length);
        assertEquals(2,transport.uploads);assertEquals(Arrays.asList(0L,0L),transport.sequences);
        assertArrayEquals(transport.packets.get(0),transport.packets.get(1));assertEquals(1,sender.nextSequence());
        sender.send(new byte[]{5},1);assertEquals(2,sender.nextSequence());sender.finish();assertTrue(sender.stopAcknowledged());assertEquals("owner-1",transport.stoppedToken);
    }

    @Test public void verificationBeforeStartPreventsControllingAnotherHost()throws Exception {
        Fake transport=new Fake();transport.current=identity("different");PhoneLiveSession sender=session(transport);
        try{sender.begin();fail("Identity mismatch");}catch(PhoneLiveSession.Rejected expected){}
        assertEquals(0,transport.starts);assertEquals(PhoneLiveSession.State.FAILED,sender.state());
    }

    @Test public void identityChangeDuringStartStopsOnlyTheNewlyOwnedSession()throws Exception {
        Fake transport=new Fake();transport.changeAfterStart=true;PhoneLiveSession sender=session(transport);
        try{sender.begin();fail("Identity changed");}catch(PhoneLiveSession.Rejected expected){}
        assertEquals(1,transport.starts);assertEquals(1,transport.stops);assertEquals("owner-1",transport.stoppedToken);
    }

    @Test public void unknownInitialStartIsNeverBlindlyRetried()throws Exception {
        Fake transport=new Fake();transport.loseStart=true;PhoneLiveSession sender=session(transport);
        try{sender.begin();fail("Start acknowledgement lost");}catch(IOException expected){}
        assertEquals(1,transport.starts);assertEquals(PhoneLiveSession.State.FAILED,sender.state());
        try{sender.resume();fail("No known token may be resumed");}catch(PhoneLiveSession.Rejected expected){}
        assertEquals(1,transport.starts);assertEquals(0,transport.stops);
    }

    @Test public void temporaryFailureResumesOnlyTheVerifiedPreviousOwner()throws Exception {
        Fake transport=new Fake();PhoneLiveSession sender=session(transport);sender.begin();transport.failUploads=true;
        try{sender.send(new byte[]{1},1);fail("Network unavailable");}catch(IOException expected){}
        assertEquals(2,transport.uploads);assertEquals(PhoneLiveSession.State.RECONNECTING,sender.state());
        sender.resume();assertEquals("owner-1",transport.previous);assertEquals(PhoneLiveSession.State.LIVE,sender.state());
        transport.failUploads=false;sender.send(new byte[]{9},1);assertEquals(0L,(long)transport.sequences.get(2));assertEquals(1,sender.nextSequence());
    }

    @Test public void replacementOrAccessRejectionDoesNotStealTheStationBack()throws Exception {
        Fake transport=new Fake();PhoneLiveSession sender=session(transport);sender.begin();transport.reject=true;
        try{sender.send(new byte[]{1},1);fail("Owner replaced");}catch(PhoneLiveSession.Rejected expected){}
        assertEquals(1,transport.uploads);assertEquals(PhoneLiveSession.State.FAILED,sender.state());
        try{sender.resume();fail("Rejected ownership must not be resumed");}catch(PhoneLiveSession.Rejected expected){}
        assertEquals(1,transport.starts);
    }

    @Test public void unsupportedHostNeverBecomesAnActiveSender()throws Exception {
        Fake transport=new Fake();transport.legacy=true;PhoneLiveSession sender=session(transport);
        try{sender.begin();fail("Host lacks sequence/resume support");}catch(PhoneLiveSession.Rejected expected){}
        assertEquals(PhoneLiveSession.State.FAILED,sender.state());assertEquals(1,transport.stops);
    }

    @Test public void cancellingInFlightUploadIsPromptAndDoesNotReplayIt()throws Exception {
        Fake transport=new Fake();PhoneLiveSession sender=session(transport);sender.begin();transport.block=true;
        Thread work=new Thread(()->{try{sender.send(new byte[]{1},1);}catch(IOException expected){}finally{sender.finish();}});
        work.start();assertTrue(transport.entered.await(2,TimeUnit.SECONDS));long before=System.nanoTime();sender.cancel();
        assertTrue("Cancellation waits for network",System.nanoTime()-before<500_000_000L);work.join(2000);
        assertFalse(work.isAlive());assertEquals(1,transport.uploads);assertEquals(PhoneLiveSession.State.STOPPED,sender.state());assertEquals(1,transport.stops);
    }

    @Test public void encodedPacketBoundsDoNotReachTransport()throws Exception {
        Fake transport=new Fake();PhoneLiveSession sender=session(transport);sender.begin();
        for(int size:new int[]{0,PhoneLiveSession.MAX_PACKET+1})try{sender.send(new byte[Math.max(1,size)],size);fail("Packet bound");}catch(IllegalArgumentException expected){}
        assertEquals(0,transport.uploads);
    }

    @Test public void mismatchedAcknowledgementFailsWithoutAdvancingTheClock()throws Exception {
        Fake transport=new Fake(){@Override public long upload(String token,long sequence,byte[] bytes){uploads++;return sequence+2;}};
        PhoneLiveSession sender=session(transport);sender.begin();
        try{sender.send(new byte[]{1},1);fail("Bad acknowledgement");}catch(PhoneLiveSession.Rejected expected){}
        assertEquals(0,sender.nextSequence());assertEquals(PhoneLiveSession.State.FAILED,sender.state());assertEquals(1,transport.uploads);
    }
}
