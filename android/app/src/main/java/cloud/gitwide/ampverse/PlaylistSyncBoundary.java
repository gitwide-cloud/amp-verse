package cloud.gitwide.ampverse;

import java.util.concurrent.Callable;

/** Selection commits and network dispatch are ordered; UI pause never waits on network I/O. */
final class PlaylistSyncBoundary {
    private final Object publication = new Object();

    static boolean selected(String active, String captured, boolean enabled) {
        return enabled && active.equals(captured);
    }

    <T> T select(Callable<T> commit) throws Exception {
        synchronized (publication) {
            return commit.call();
        }
    }

    <T> T dispatch(Callable<Boolean> stillSelected, Callable<T> send) throws Exception {
        synchronized (publication) {
            if (!stillSelected.call()) return null;
            return send.call();
        }
    }
}
