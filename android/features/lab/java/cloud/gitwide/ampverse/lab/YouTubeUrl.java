package cloud.gitwide.ampverse.lab;

import java.net.URI;
import java.net.URLDecoder;
import java.nio.charset.StandardCharsets;
import java.util.Locale;

/** Deliberately accepts one public YouTube video, never an arbitrary extractor URL. */
public final class YouTubeUrl {
    private YouTubeUrl() {}
    public static String normalize(String value) {
        try {
            URI uri = new URI(value.trim());
            String host = uri.getHost() == null ? "" : uri.getHost().toLowerCase(Locale.ROOT);
            if (!"https".equalsIgnoreCase(uri.getScheme()) || uri.getUserInfo() != null || uri.getPort() != -1)
                throw new IllegalArgumentException("Paste a secure YouTube video URL.");
            String id = null, path = uri.getPath();
            if (host.equals("youtu.be")) id = path.substring(1);
            else if (host.equals("youtube.com") || host.equals("www.youtube.com") || host.equals("m.youtube.com") || host.equals("music.youtube.com")) {
                if (path.equals("/watch")) {
                    for (String part : (uri.getRawQuery() == null ? "" : uri.getRawQuery()).split("&")) {
                        String[] pair = part.split("=", 2);
                        if (pair.length == 2 && pair[0].equals("v")) {
                            if (id != null) throw new IllegalArgumentException("Use a URL for one video.");
                            id = URLDecoder.decode(pair[1], StandardCharsets.UTF_8.name());
                        }
                    }
                } else if (path.startsWith("/shorts/") || path.startsWith("/live/")) id = path.substring(path.lastIndexOf('/') + 1);
                else if (path.equals("/playlist")) throw new IllegalArgumentException("Choose a video in the playlist, then paste its video URL.");
            }
            if (id == null || !id.matches("[A-Za-z0-9_-]{11}")) throw new IllegalArgumentException("Paste a YouTube video link, not a channel or playlist-only link.");
            return "https://www.youtube.com/watch?v=" + id;
        } catch (IllegalArgumentException error) { throw error; }
        catch (Exception error) { throw new IllegalArgumentException("That YouTube URL is not valid."); }
    }
}
