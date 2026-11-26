package cloud.gitwide.ampverse;

import java.net.URI;
import java.net.URLDecoder;

/** Distinguishes an AMP receiver from legitimate owned file/internet-radio playback. */
final class PhoneSourceIdentity {
  static boolean receiver(String address, String selectedBase) {
    if (address == null) return false;
    try {
      URI source = new URI(address);
      String path = source.getPath();
      if ("/amp-protected.mp3".equals(path) || "/api/audio/hls/master.m3u8".equals(path))
        return true;
      if (!"/listen.mp3".equals(path)) return false;
      String query = source.getRawQuery();
      if (query != null)
        for (String part : query.split("&")) {
          String[] entry = part.split("=", 2);
          if (entry.length == 2
              && "amp_receiver".equals(URLDecoder.decode(entry[0], "UTF-8"))
              && "1".equals(URLDecoder.decode(entry[1], "UTF-8"))) return true;
        }
      if (selectedBase == null || selectedBase.isEmpty()) return false;
      URI selected = new URI(selectedBase);
      return source.getHost() != null
          && source.getHost().equalsIgnoreCase(selected.getHost())
          && source.getScheme().equalsIgnoreCase(selected.getScheme())
          && port(source) == port(selected);
    } catch (Exception malformed) {
      return false;
    }
  }

  private static int port(URI uri) {
    return uri.getPort() < 0
        ? ("https".equalsIgnoreCase(uri.getScheme()) ? 443 : 80)
        : uri.getPort();
  }

  private PhoneSourceIdentity() {}
}
