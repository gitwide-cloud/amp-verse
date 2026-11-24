package cloud.gitwide.ampverse;

import static org.junit.Assert.*;

import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import okhttp3.*;
import okio.Timeout;
import org.junit.Test;

public final class PhoneLiveHttpTransportTest {
  static final class PausedCall implements Call {
    final Call delegate;
    final CountDownLatch entered, release;

    PausedCall(Call delegate, CountDownLatch entered, CountDownLatch release) {
      this.delegate = delegate;
      this.entered = entered;
      this.release = release;
    }

    public Request request() {
      return delegate.request();
    }

    public Response execute() throws IOException {
      entered.countDown();
      waitFor(release);
      return delegate.execute();
    }

    public void enqueue(Callback callback) {
      delegate.enqueue(callback);
    }

    public void cancel() {
      delegate.cancel();
    }

    public boolean isExecuted() {
      return delegate.isExecuted();
    }

    public boolean isCanceled() {
      return delegate.isCanceled();
    }

    public Timeout timeout() {
      return delegate.timeout();
    }

    public Call clone() {
      return delegate.clone();
    }
  }

  static void waitFor(CountDownLatch gate) throws IOException {
    try {
      if (!gate.await(2, TimeUnit.SECONDS)) throw new IOException("Fixture barrier timed out");
    } catch (InterruptedException error) {
      Thread.currentThread().interrupt();
      throw new IOException(error);
    }
  }

  @Test
  public void cancelBeforeCallPublicationPreventsNewRequestsButPermitsOwnedStop() throws Exception {
    cancelledRequestNeverReachesServer(true);
  }

  @Test
  public void cancelAfterPublicationBeforeHttpEngineCreationIsStickyAndPermitsOwnedStop()
      throws Exception {
    cancelledRequestNeverReachesServer(false);
  }

  private void cancelledRequestNeverReachesServer(boolean factoryBarrier) throws Exception {
    ServerSocket server = new ServerSocket(0, 1, InetAddress.getByName("127.0.0.1"));
    server.setSoTimeout(4000);
    AtomicInteger accepted = new AtomicInteger();
    AtomicReference<String> request = new AtomicReference<>(), owner = new AtomicReference<>();
    AtomicReference<Throwable> hostFailure = new AtomicReference<>();
    Thread host =
        new Thread(
            () -> {
              try (Socket socket = server.accept()) {
                accepted.incrementAndGet();
                socket.setSoTimeout(2000);
                BufferedReader input =
                    new BufferedReader(
                        new InputStreamReader(socket.getInputStream(), StandardCharsets.US_ASCII));
                request.set(input.readLine());
                String line;
                while ((line = input.readLine()) != null && !line.isEmpty())
                  if (line.toLowerCase(java.util.Locale.ROOT).startsWith("x-amp-live-session:"))
                    owner.set(line.substring(line.indexOf(':') + 1).trim());
                socket
                    .getOutputStream()
                    .write(
                        "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n"
                            .getBytes(StandardCharsets.US_ASCII));
                socket.getOutputStream().flush();
              } catch (Throwable error) {
                hostFailure.set(error);
              }
            },
            "owned-cancel-fixture");
    host.setDaemon(true);
    host.start();
    OkHttpClient client =
        new OkHttpClient.Builder()
            .retryOnConnectionFailure(false)
            .callTimeout(2, TimeUnit.SECONDS)
            .build();
    CountDownLatch entered = new CountDownLatch(1), release = new CountDownLatch(1);
    AtomicBoolean first = new AtomicBoolean(true);
    PhoneLiveHttpTransport transport =
        new PhoneLiveHttpTransport(
            "http://127.0.0.1:" + server.getLocalPort(),
            "",
            new PhoneLiveSession.Identity("server", "station", "library"),
            value -> {
              Call actual = client.newCall(value);
              if (first.compareAndSet(true, false)) {
                if (factoryBarrier) {
                  entered.countDown();
                  try {
                    waitFor(release);
                  } catch (IOException error) {
                    throw new IllegalStateException(error);
                  }
                } else return new PausedCall(actual, entered, release);
              }
              return actual;
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
      assertTrue(entered.await(1, TimeUnit.SECONDS));
      transport.abort();
      release.countDown();
      start.get(2, TimeUnit.SECONDS);
      assertEquals("Cancelled start cannot create any connection", 0, accepted.get());
      try {
        transport.upload("owned", 0, new byte[] {1});
        fail("Cancelled upload succeeded");
      } catch (IOException expected) {
      }
      assertEquals("Cancellation stays sticky for subsequent upload", 0, accepted.get());
      transport.stop("owned");
      host.join(3000);
      assertFalse(host.isAlive());
      assertNull(hostFailure.get());
      assertEquals("Only explicit identity-guarded cleanup reaches the host", 1, accepted.get());
      assertEquals("POST /api/live/stop HTTP/1.1", request.get());
      assertEquals("owned", owner.get());
    } finally {
      release.countDown();
      worker.shutdownNow();
      server.close();
      host.join(1000);
      client.dispatcher().executorService().shutdownNow();
      client.connectionPool().evictAll();
    }
  }
}
