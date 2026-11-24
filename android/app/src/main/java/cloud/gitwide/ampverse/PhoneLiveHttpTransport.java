package cloud.gitwide.ampverse;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.TimeUnit;
import okhttp3.Call;
import okhttp3.MediaType;
import okhttp3.OkHttpClient;
import okhttp3.Request;
import okhttp3.RequestBody;
import okhttp3.Response;
import okhttp3.ResponseBody;
import org.json.JSONObject;

/** Selected-host requests use sticky per-call cancellation; credentials never enter URLs. */
final class PhoneLiveHttpTransport implements PhoneLiveSession.Transport {
  private static final MediaType AUDIO = MediaType.parse("application/octet-stream");

  private static final class Client {
    static final OkHttpClient INSTANCE =
        new OkHttpClient.Builder()
            .followRedirects(false)
            .followSslRedirects(false)
            // A lost start acknowledgement must never trigger a hidden second claim.
            .retryOnConnectionFailure(false)
            .connectTimeout(1500, TimeUnit.MILLISECONDS)
            .readTimeout(2000, TimeUnit.MILLISECONDS)
            .writeTimeout(2000, TimeUnit.MILLISECONDS)
            .callTimeout(4500, TimeUnit.MILLISECONDS)
            .build();
  }

  private final URI base;
  private final String password;
  private final PhoneLiveSession.Identity selected;
  private final Call.Factory calls;
  private final Object publication = new Object();
  private Call current;
  private boolean currentCleanup;
  private volatile boolean aborted;

  PhoneLiveHttpTransport(String address, String password, PhoneLiveSession.Identity selected)
      throws IOException {
    this(address, password, selected, Client.INSTANCE);
  }

  PhoneLiveHttpTransport(
      String address, String password, PhoneLiveSession.Identity selected, Call.Factory calls)
      throws IOException {
    try {
      URI uri = new URI(address);
      if (!("http".equalsIgnoreCase(uri.getScheme()) || "https".equalsIgnoreCase(uri.getScheme()))
          || uri.getHost() == null
          || uri.getUserInfo() != null
          || uri.getQuery() != null
          || uri.getFragment() != null
          || !(uri.getPath().isEmpty() || uri.getPath().equals("/")))
        throw new IOException("Invalid selected host");
      base = new URI(uri.getScheme(), null, uri.getHost(), uri.getPort(), "/", null, null);
    } catch (java.net.URISyntaxException error) {
      throw new IOException("Invalid selected host", error);
    }
    if (selected == null
        || password == null
        || password.contains("\r")
        || password.contains("\n")
        || calls == null) throw new IOException("Selected host credentials unavailable");
    this.password = password;
    this.selected = selected;
    this.calls = calls;
  }

  public PhoneLiveSession.Identity verify() throws IOException {
    Reply reply = request("/api/status", false, null, null, null);
    try {
      JSONObject data = new JSONObject(reply.body);
      return new PhoneLiveSession.Identity(
          data.getString("server_id"), data.getString("station_id"), data.getString("library_id"));
    } catch (Exception error) {
      throw new PhoneLiveSession.Rejected("Host did not provide a usable identity");
    }
  }

  public PhoneLiveSession.Started start(String previous) throws IOException {
    Reply reply = request("/api/live/start", true, null, previous, null);
    try {
      JSONObject data = new JSONObject(reply.body);
      return new PhoneLiveSession.Started(
          data.getString("session"),
          data.getLong("next_sequence"),
          data.optBoolean("upload_sequence"),
          data.optBoolean("guarded_resume"));
    } catch (Exception error) {
      throw new PhoneLiveSession.Rejected("Host did not provide reliable live ownership");
    }
  }

  public long upload(String token, long sequence, byte[] bytes) throws IOException {
    Reply reply = request("/api/live/audio", true, bytes, null, new Owner(token, sequence));
    try {
      return Long.parseLong(reply.nextSequence);
    } catch (Exception error) {
      throw new PhoneLiveSession.Rejected("Host omitted the live upload acknowledgement");
    }
  }

  public void stop(String token) throws IOException {
    request("/api/live/stop", true, null, null, new Owner(token, -1));
  }

  public void abort() {
    Call call;
    synchronized (publication) {
      aborted = true;
      call = currentCleanup ? null : current;
    }
    if (call != null) call.cancel();
  }

  private void checkActive(boolean cleanup) throws PhoneLiveSession.Rejected {
    if (aborted && !cleanup) throw new PhoneLiveSession.Rejected("Phone broadcast cancelled");
  }

  private Reply request(String route, boolean post, byte[] bytes, String previous, Owner owner)
      throws IOException {
    boolean cleanup = route.equals("/api/live/stop");
    checkActive(cleanup);
    Request.Builder builder =
        new Request.Builder()
            .url(base.resolve(route).toString())
            .header("x-amp-server", selected.server)
            .header("x-amp-station", selected.station)
            .header("x-amp-library", selected.library);
    if (!password.isEmpty()) builder.header("x-amp-password", password);
    if (previous != null) builder.header("x-amp-previous-session", previous);
    if (owner != null) {
      builder.header("x-amp-live-session", owner.token);
      if (owner.sequence >= 0) builder.header("x-amp-live-sequence", Long.toString(owner.sequence));
    }
    if (post) builder.post(RequestBody.create(bytes == null ? new byte[0] : bytes, AUDIO));
    Call call = calls.newCall(builder.build());
    boolean rejected;
    synchronized (publication) {
      rejected = aborted && !cleanup;
      if (!rejected) {
        current = call;
        currentCleanup = cleanup;
      }
    }
    if (rejected) {
      call.cancel();
      throw new PhoneLiveSession.Rejected("Phone broadcast cancelled");
    }
    // Call.cancel is sticky even if cancellation lands before execute creates its HTTP engine.
    // execute/response I/O is never inside the publication lock.
    try (Response response = call.execute()) {
      int code = response.code();
      if (code < 200 || code >= 300) {
        if (code >= 400 && code < 500)
          throw new PhoneLiveSession.Rejected(
              code == 409
                  ? "Live owner or upload sequence changed"
                  : "Host rejected live access; reconnect explicitly");
        if (code >= 300 && code < 400)
          throw new PhoneLiveSession.Rejected("Live host redirected; select it explicitly");
        throw new IOException("Live host temporarily unavailable");
      }
      String next = response.header("x-amp-live-next-sequence");
      if (code == 204) return new Reply("", next);
      ResponseBody body = response.body();
      if (body == null) throw new IOException("Live host response body unavailable");
      try (InputStream input = body.byteStream();
          ByteArrayOutputStream output = new ByteArrayOutputStream()) {
        byte[] buffer = new byte[4096];
        int count;
        while ((count = input.read(buffer)) != -1) {
          if (output.size() + count > 65536)
            throw new PhoneLiveSession.Rejected("Live host response exceeds limit");
          output.write(buffer, 0, count);
        }
        return new Reply(new String(output.toByteArray(), StandardCharsets.UTF_8), next);
      }
    } finally {
      synchronized (publication) {
        if (current == call) {
          current = null;
          currentCleanup = false;
        }
      }
    }
  }

  private static final class Owner {
    final String token;
    final long sequence;

    Owner(String token, long sequence) {
      this.token = token;
      this.sequence = sequence;
    }
  }

  private static final class Reply {
    final String body, nextSequence;

    Reply(String body, String nextSequence) {
      this.body = body;
      this.nextSequence = nextSequence;
    }
  }
}
