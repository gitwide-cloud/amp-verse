package cloud.gitwide.ampverse;

import java.util.Arrays;

/**
 * The phone's owned master, before both local output and broadcast encoding.
 *
 * All input is interleaved, signed 16-bit, stereo PCM at 48 kHz. Decoder adapters
 * resample before admission. Cue outputs are deliberately not master sources.
 * No decoder, encoder, network operation or disk access runs in render().
 */
final class OwnedPcmMixer {
    static final int RATE = 48000;
    static final int CHANNELS = 2;
    static final int MAX_VOICES = 24;
    static final int MAX_RENDER_FRAMES = 1920;
    static final int DEFAULT_QUEUE_FRAMES = RATE / 2;
    private static final int MARKERS = 128;

    enum Owner { PLAYER, DJ }
    enum Source { PLAYER, DECK_A, DECK_B, SCRATCH_A, SCRATCH_B, SAMPLER, PRIVATE_CUE }
    interface Fanout {
        /** Non-blocking; copies the samples it retains before returning. */
        boolean offer(short[] pcm, int frames, long masterFrame);
    }

    private final Voice[] voices = new Voice[MAX_VOICES];
    private final float[] sum = new float[MAX_RENDER_FRAMES * CHANNELS];
    private Owner owner = Owner.PLAYER;
    private Fanout fanout;
    private long frame;
    private long rejectedFanoutFrames;

    synchronized Voice voice(Source source) {
        return voice(source, DEFAULT_QUEUE_FRAMES);
    }

    synchronized Voice voice(Source source, int capacityFrames) {
        if (source == null || source == Source.PRIVATE_CUE) {
            throw new IllegalArgumentException("Private cue never enters the master");
        }
        if (capacityFrames < 1 || capacityFrames > RATE) {
            throw new IllegalArgumentException("PCM queue must be bounded to one second");
        }
        for (int i = 0; i < voices.length; i++) {
            if (voices[i] == null) {
                Voice voice = new Voice(this, source, capacityFrames);
                voices[i] = voice;
                return voice;
            }
        }
        throw new IllegalStateException("Master voice limit reached");
    }

    synchronized void owner(Owner next) {
        if (next == null) throw new IllegalArgumentException("Owner required");
        owner = next;
    }

    synchronized Owner owner() { return owner; }
    synchronized long frames() { return frame; }
    synchronized long rejectedFanoutFrames() { return rejectedFanoutFrames; }

    /** One receiver/sender subscription; replacing it never changes local audio. */
    synchronized void fanout(Fanout next) {
        fanout = next;
        rejectedFanoutFrames = 0;
    }

    synchronized void remove(Voice voice) {
        for (int i = 0; i < voices.length; i++) {
            if (voices[i] == voice) {
                voice.closed = true;
                voice.resetLocked();
                voices[i] = null;
                return;
            }
        }
    }

    /**
     * Produces one master block. The very same samples go to local AudioTrack
     * and the optional encoder subscription; rejection only disconnects export.
     */
    synchronized void render(short[] output, int frames) {
        if (frames < 0 || frames > MAX_RENDER_FRAMES || output.length < frames * CHANNELS) {
            throw new IllegalArgumentException("Invalid master block");
        }
        Arrays.fill(sum, 0, frames * CHANNELS, 0f);
        boolean scratchA = active(Source.SCRATCH_A);
        boolean scratchB = active(Source.SCRATCH_B);
        for (Voice voice : voices) {
            if (voice == null || !voice.active || !permitted(voice.source)) continue;
            if ((voice.source == Source.DECK_A && scratchA)
                    || (voice.source == Source.DECK_B && scratchB)) continue;
            int count = Math.min(frames, voice.size);
            long mediaStart = voice.consumed;
            for (int i = 0; i < count; i++) {
                int source = voice.read * CHANNELS;
                sum[i * CHANNELS] += voice.pcm[source] * voice.gain;
                sum[i * CHANNELS + 1] += voice.pcm[source + 1] * voice.gain;
                voice.read = (voice.read + 1) % voice.capacity;
            }
            voice.size -= count;
            voice.consumed += count;
            if (count > 0) voice.marker(frame, mediaStart, count);
        }
        for (int i = 0; i < frames * CHANNELS; i++) {
            output[i] = (short) Math.max(Short.MIN_VALUE, Math.min(Short.MAX_VALUE,
                    Math.round(sum[i])));
        }
        if (fanout != null && frames > 0) {
            boolean accepted;
            try { accepted = fanout.offer(output, frames, frame); }
            catch (RuntimeException error) { accepted = false; }
            if (!accepted) {
                rejectedFanoutFrames += frames;
                // A congested network must never block or corrupt the local master.
                fanout = null;
            }
        }
        frame += frames;
    }

    private boolean active(Source source) {
        if (owner != Owner.DJ) return false;
        for (Voice voice : voices) {
            if (voice != null && voice.source == source && voice.active) return true;
        }
        return false;
    }

    private boolean permitted(Source source) {
        return owner == Owner.PLAYER ? source == Source.PLAYER : source != Source.PLAYER;
    }

    /** One generation per media load/seek; stale decoder work cannot enter it. */
    static final class Voice {
        private final OwnedPcmMixer mixer;
        final Source source;
        private final int capacity;
        private final short[] pcm;
        private final long[] outputStart = new long[MARKERS];
        private final long[] mediaStart = new long[MARKERS];
        private final int[] markerFrames = new int[MARKERS];
        private int read, size, markerCount, markerNext;
        private long generation = 1, consumed, lastOutputFrame;
        private float gain = 1f;
        private boolean active, ended, closed;

        private Voice(OwnedPcmMixer mixer, Source source, int capacity) {
            this.mixer = mixer;
            this.source = source;
            this.capacity = capacity;
            pcm = new short[capacity * CHANNELS];
        }

        long generation() { synchronized (mixer) { return generation; } }
        int queuedFrames() { synchronized (mixer) { return size; } }
        int writableFrames() { synchronized (mixer) { return closed ? 0 : capacity - size; } }

        void gain(float value) {
            if (!Float.isFinite(value) || value < 0 || value > 2f) {
                throw new IllegalArgumentException("Voice gain must be between zero and two");
            }
            synchronized (mixer) { gain = value; }
        }

        void play() { synchronized (mixer) { if (!closed) active = true; } }
        void pause() { synchronized (mixer) { active = false; } }
        void end() { synchronized (mixer) { ended = true; } }

        /** Does not consume a partial frame or wait for the rendering thread. */
        int write(short[] input, int offsetFrames, int frames, long epoch) {
            if (offsetFrames < 0 || frames < 0
                    || ((long) offsetFrames + frames) * CHANNELS > input.length) {
                throw new IllegalArgumentException("Invalid PCM range");
            }
            synchronized (mixer) {
                if (closed || epoch != generation || ended) return 0;
                int accepted = Math.min(frames, capacity - size);
                for (int i = 0; i < accepted; i++) {
                    int at = ((read + size + i) % capacity) * CHANNELS;
                    pcm[at] = input[(offsetFrames + i) * CHANNELS];
                    pcm[at + 1] = input[(offsetFrames + i) * CHANNELS + 1];
                }
                size += accepted;
                return accepted;
            }
        }

        long flush() { synchronized (mixer) { return resetLocked(); } }
        void close() { mixer.remove(this); }

        private long resetLocked() {
            if (generation == Long.MAX_VALUE) throw new IllegalStateException("PCM generation exhausted");
            generation++;
            read = size = markerCount = markerNext = 0;
            consumed = 0;
            lastOutputFrame = mixer.frame;
            ended = false;
            return generation;
        }

        private void marker(long output, long media, int count) {
            outputStart[markerNext] = output;
            mediaStart[markerNext] = media;
            markerFrames[markerNext] = count;
            markerNext = (markerNext + 1) % MARKERS;
            markerCount = Math.min(MARKERS, markerCount + 1);
            lastOutputFrame = output + count;
        }

        /** Media frames actually presented, including pauses and other voices. */
        long positionFrames(long presentedMasterFrame) {
            synchronized (mixer) {
                long position = 0;
                int oldest = (markerNext - markerCount + MARKERS) % MARKERS;
                if (markerCount > 0 && mediaStart[oldest] > 0
                        && presentedMasterFrame < outputStart[oldest]) return -1;
                for (int i = 0; i < markerCount; i++) {
                    int at = (oldest + i) % MARKERS;
                    if (presentedMasterFrame < outputStart[at]) return position;
                    int elapsed = (int) Math.min(markerFrames[at],
                            presentedMasterFrame - outputStart[at]);
                    position = mediaStart[at] + elapsed;
                    if (elapsed < markerFrames[at]) return position;
                }
                return position;
            }
        }

        boolean drained(long presentedMasterFrame) {
            synchronized (mixer) {
                return ended && size == 0 && presentedMasterFrame >= lastOutputFrame;
            }
        }
    }
}
