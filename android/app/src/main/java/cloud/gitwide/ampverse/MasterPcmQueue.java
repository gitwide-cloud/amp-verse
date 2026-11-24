package cloud.gitwide.ampverse;

/** A bounded post-master fanout; never blocks an audio callback or writes a file. */
final class MasterPcmQueue implements OwnedPcmMixer.Fanout {
    private final short[] pcm;
    private int read, size;
    private boolean closed;
    private long nextMasterFrame = -1;

    MasterPcmQueue(int capacityFrames) {
        if (capacityFrames < 1 || capacityFrames > OwnedPcmMixer.RATE) {
            throw new IllegalArgumentException("Export queue must be bounded to one second");
        }
        pcm = new short[capacityFrames * OwnedPcmMixer.CHANNELS];
    }

    @Override public synchronized boolean offer(short[] input, int frames, long masterFrame) {
        if (closed || frames < 0 || (long) frames * 2 > input.length
                || frames > (pcm.length - size) / 2
                || (nextMasterFrame >= 0 && masterFrame != nextMasterFrame)) return false;
        for (int i = 0; i < frames * 2; i++) pcm[(read + size + i) % pcm.length] = input[i];
        size += frames * 2;
        nextMasterFrame = masterFrame + frames;
        return true;
    }

    synchronized int read(short[] output, int maxFrames) {
        if (maxFrames < 0 || (long) maxFrames * 2 > output.length) {
            throw new IllegalArgumentException("Invalid PCM read range");
        }
        int frames = Math.min(maxFrames, size / 2);
        for (int i = 0; i < frames * 2; i++) output[i] = pcm[(read + i) % pcm.length];
        read = (read + frames * 2) % pcm.length;
        size -= frames * 2;
        return frames;
    }

    synchronized int frames() { return size / 2; }
    synchronized void close() { closed = true; read = size = 0; }
}
