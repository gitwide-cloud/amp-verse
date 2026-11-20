package cloud.gitwide.ampverse;

import org.junit.Test;
import static org.junit.Assert.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;

public class PlaylistSyncBoundaryTest {
    @Test public void selectingBDuringAFetchKeepsAOutboxWithoutSending() throws Exception {
        PlaylistSyncBoundary boundary=new PlaylistSyncBoundary();
        AtomicReference<String> active=new AtomicReference<>("A");
        AtomicInteger sends=new AtomicInteger();
        AtomicReference<String> outbox=new AtomicReference<>("exact-unsent-A-operation");
        CountDownLatch fetching=new CountDownLatch(1),releaseFetch=new CountDownLatch(1);
        ExecutorService worker=Executors.newSingleThreadExecutor();
        try {
            Future<String> oldCycle=worker.submit(()->{
                fetching.countDown();assertTrue(releaseFetch.await(5,TimeUnit.SECONDS));
                return boundary.dispatch(()->PlaylistSyncBoundary.selected(active.get(),"A",true),()->{sends.incrementAndGet();outbox.set(null);return "sent";});
            });
            assertTrue(fetching.await(5,TimeUnit.SECONDS));
            boundary.select(()->{active.set("B");return null;});
            releaseFetch.countDown();
            assertNull(oldCycle.get(5,TimeUnit.SECONDS));
            assertEquals(0,sends.get());assertEquals("exact-unsent-A-operation",outbox.get());
        } finally {releaseFetch.countDown();worker.shutdownNow();}
    }

    @Test public void bindingSelectionCannotCommitWhileOldPublicationIsInFlight() throws Exception {
        PlaylistSyncBoundary boundary=new PlaylistSyncBoundary();
        AtomicReference<String> active=new AtomicReference<>("A");
        CountDownLatch sending=new CountDownLatch(1),finish=new CountDownLatch(1),selecting=new CountDownLatch(1);
        ExecutorService workers=Executors.newFixedThreadPool(2);
        try {
            Future<String> publication=workers.submit(()->boundary.dispatch(()->true,()->{sending.countDown();assertTrue(finish.await(5,TimeUnit.SECONDS));assertEquals("A",active.get());return "ack";}));
            assertTrue(sending.await(5,TimeUnit.SECONDS));
            Future<?> selection=workers.submit(()->{selecting.countDown();return boundary.select(()->{active.set("B");return null;});});
            assertTrue(selecting.await(5,TimeUnit.SECONDS));try{selection.get(100,TimeUnit.MILLISECONDS);fail("Selection must wait for the in-flight dispatch");}catch(TimeoutException expected){}assertEquals("A",active.get());
            finish.countDown();assertEquals("ack",publication.get(5,TimeUnit.SECONDS));selection.get(5,TimeUnit.SECONDS);assertEquals("B",active.get());
        } finally {finish.countDown();workers.shutdownNow();}
    }

    @Test public void pauseBeforeStagingOrDispatchNeverSendsThePendingRequest() throws Exception {
        PlaylistSyncBoundary boundary=new PlaylistSyncBoundary();
        AtomicBoolean enabled=new AtomicBoolean(false);
        AtomicInteger staged=new AtomicInteger(),sends=new AtomicInteger();
        if(PlaylistSyncBoundary.selected("A","A",enabled.get()))staged.incrementAndGet();
        assertNull(boundary.dispatch(()->PlaylistSyncBoundary.selected("A","A",enabled.get()),()->{sends.incrementAndGet();return "sent";}));
        assertEquals(0,staged.get());assertEquals(0,sends.get());
    }
}
