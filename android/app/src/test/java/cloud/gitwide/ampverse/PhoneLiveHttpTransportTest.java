package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import java.io.*;
import java.net.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import org.junit.Test;

public final class PhoneLiveHttpTransportTest {
  static final class Connection extends HttpURLConnection {
    final AtomicInteger writes;

    Connection(URL u, AtomicInteger writes) {
      super(u);
      this.writes = writes;
    }

    public void disconnect() {}

    public boolean usingProxy() {
      return false;
    }

    public void connect() {}

    public OutputStream getOutputStream() {
      writes.incrementAndGet();
      return new ByteArrayOutputStream();
    }

    public int getResponseCode() {
      return 204;
    }
  }

  @Test
  public void
      cancelBetweenConnectionCreationAndPublicationPreventsAnyNewStartOrUploadButAllowsOwnedStop()
          throws Exception {
    AtomicInteger writes = new AtomicInteger();
    CountDownLatch creating = new CountDownLatch(1), publish = new CountDownLatch(1);
    AtomicBoolean first = new AtomicBoolean(true);
    PhoneLiveHttpTransport transport =
        new PhoneLiveHttpTransport(
            "http://192.168.1.2:8787",
            "",
            new PhoneLiveSession.Identity("server", "station", "library"),
            url -> {
              if (first.compareAndSet(true, false)) {
                creating.countDown();
                try {
                  if (!publish.await(2, TimeUnit.SECONDS))
                    throw new IOException("Fixture creation timed out");
                } catch (InterruptedException e) {
                  throw new IOException(e);
                }
              }
              return new Connection(url, writes);
            });
    ExecutorService worker = Executors.newSingleThreadExecutor();
    try {
      Future<?> start =
          worker.submit(
              () -> {
                try {
                  transport.start(null);
                  fail("Cancelled start succeeded");
                } catch (IOException expected) {
                }
              });
      assertTrue(creating.await(1, TimeUnit.SECONDS));
      transport.abort();
      publish.countDown();
      start.get(2, TimeUnit.SECONDS);
      assertEquals("Cancelled start cannot reach the wire", 0, writes.get());
      try {
        transport.upload("owned", 0, new byte[] {1});
        fail("Cancelled upload succeeded");
      } catch (IOException expected) {
      }
      assertEquals("Cancellation is sticky for later upload requests", 0, writes.get());
      transport.stop("owned");
      assertEquals("Guarded cleanup stop is still allowed", 1, writes.get());
    } finally {
      publish.countDown();
      worker.shutdownNow();
    }
  }
}
