package cloud.gitwide.ampverse;

import java.util.Arrays;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Owned 48 kHz stereo master. One render thread consumes SPSC voice rings;
 * render never acquires a UI/decoder/encoder monitor or invokes external code.
 * Final hard saturation is a safety guard, not a transparent limiter: callers
 * supply headroom and clippedSamples exposes overload.
 */
final class OwnedPcmMixer {
  static final int RATE=48000, CHANNELS=2, MAX_VOICES=24;
  static final int MAX_RENDER_FRAMES=1920, DEFAULT_QUEUE_FRAMES=RATE/2;
  private static final int MARKERS=128;
  enum Owner { PLAYER,DJ }
  enum Source { PLAYER,DECK_A,DECK_B,SCRATCH_A,SCRATCH_B,SAMPLER,PRIVATE_CUE }
  private final Voice[] registered=new Voice[MAX_VOICES];
  private volatile Voice[] voices=registered.clone();
  private final float[] sum=new float[MAX_RENDER_FRAMES*CHANNELS];
  private volatile Owner owner=Owner.PLAYER;
  private volatile MasterPcmQueue fanout,observedFanout;
  private volatile long frame,clippedSamples;
  private volatile float masterGain=1f;

  Voice voice(Source source){return voice(source,DEFAULT_QUEUE_FRAMES);}
  /** Registration runs on a control thread, never the render callback. */
  synchronized Voice voice(Source source,int capacity){
    if(source==null||source==Source.PRIVATE_CUE)throw new IllegalArgumentException("Private cue never enters master");
    if(capacity<1||capacity>RATE)throw new IllegalArgumentException("PCM queue bounded to one second");
    for(int i=0;i<registered.length;i++)if(registered[i]==null){Voice result=new Voice(this,source,capacity);registered[i]=result;voices=registered.clone();return result;}
    throw new IllegalStateException("Master voice limit reached");
  }
  void owner(Owner next){if(next==null)throw new IllegalArgumentException("Owner required");owner=next;}
  Owner owner(){return owner;}
  long frames(){return frame;}
  long clippedSamples(){return clippedSamples;}
  void masterGain(float value){if(!Float.isFinite(value)||value<0||value>1)throw new IllegalArgumentException("Master gain must be zero to one");masterGain=value;}
  void fanout(MasterPcmQueue next){observedFanout=next;fanout=next;}
  long rejectedFanoutFrames(){MasterPcmQueue queue=observedFanout;return queue==null?0:queue.rejectedFrames();}
  private synchronized void remove(Voice voice){
    voice.closed=true;voice.active=false;
    for(int i=0;i<registered.length;i++)if(registered[i]==voice){registered[i]=null;voices=registered.clone();return;}
  }

  /** Bounded copies/arithmetic only. Exactly one caller owns the render clock. */
  void render(short[] output,int frames){
    if(frames<0||frames>MAX_RENDER_FRAMES||output.length<frames*CHANNELS)throw new IllegalArgumentException("Invalid master block");
    Arrays.fill(sum,0,frames*CHANNELS,0f);
    Voice[] snapshot=voices;Owner selected=owner;long start=frame;
    boolean scratchA=active(snapshot,Source.SCRATCH_A,selected),scratchB=active(snapshot,Source.SCRATCH_B,selected);
    for(Voice voice:snapshot){
      if(voice==null||voice.closed||!voice.active||!permitted(voice.source,selected))continue;
      if((voice.source==Source.DECK_A&&scratchA)||(voice.source==Source.DECK_B&&scratchB))continue;
      Buffer b=voice.buffer;long read=b.read,write=b.write;int count=(int)Math.min(frames,write-read);float gain=voice.gain;long mediaStart=b.consumed;
      for(int i=0;i<count;i++){int at=(int)((read+i)%b.capacity)*2;sum[i*2]+=b.pcm[at]*gain;sum[i*2+1]+=b.pcm[at+1]*gain;}
      b.consumed+=count;if(count>0)b.marker(start,mediaStart,count);b.read=read+count;
    }
    float gain=masterGain;long clipped=clippedSamples;
    for(int i=0;i<frames*2;i++){float value=sum[i]*gain;if(value>Short.MAX_VALUE||value<Short.MIN_VALUE)clipped++;output[i]=(short)Math.max(Short.MIN_VALUE,Math.min(Short.MAX_VALUE,Math.round(value)));}
    clippedSamples=clipped;
    MasterPcmQueue queue=fanout;
    if(queue!=null&&frames>0&&!queue.offer(output,frames,start)){queue.reject(frames);if(fanout==queue)fanout=null;}
    frame=start+frames;
  }
  private static boolean active(Voice[] voices,Source source,Owner owner){
    if(owner!=Owner.DJ)return false;
    for(Voice v:voices)if(v!=null&&!v.closed&&v.source==source&&v.active)return true;
    return false;
  }
  private static boolean permitted(Source source,Owner owner){return owner==Owner.PLAYER?source==Source.PLAYER:source!=Source.PLAYER;}

  static final class Voice {
    private final OwnedPcmMixer mixer;
    final Source source;
    private final int capacity;
    private final AtomicLong epoch=new AtomicLong(1);
    private volatile Buffer buffer;
    private volatile float gain=1;
    private volatile boolean active,closed;
    private Voice(OwnedPcmMixer mixer,Source source,int capacity){this.mixer=mixer;this.source=source;this.capacity=capacity;buffer=new Buffer(capacity,1,mixer.frame);}
    long generation(){return buffer.generation;}
    int queuedFrames(){Buffer b=buffer;long read=b.read;return (int)Math.max(0,Math.min(capacity,b.write-read));}
    int writableFrames(){return closed?0:capacity-queuedFrames();}
    void gain(float value){if(!Float.isFinite(value)||value<0||value>2)throw new IllegalArgumentException("Voice gain must be zero to two");gain=value;}
    void play(){if(!closed)active=true;}
    void pause(){active=false;}
    void end(){buffer.ended=true;}

    /** One producer per voice; partial writes provide decoder backpressure. */
    int write(short[] input,int offsetFrames,int frames,long generation){
      if(offsetFrames<0||frames<0||((long)offsetFrames+frames)*2>input.length)throw new IllegalArgumentException("Invalid PCM range");
      Buffer b=buffer;if(closed||generation!=b.generation||b.ended)return 0;
      long write=b.write,read=b.read;int accepted=(int)Math.min(frames,capacity-(write-read));
      for(int i=0;i<accepted;i++){int at=(int)((write+i)%capacity)*2;b.pcm[at]=input[(offsetFrames+i)*2];b.pcm[at+1]=input[(offsetFrames+i)*2+1];}
      if(closed||buffer!=b)return 0;b.write=write+accepted;return accepted;
    }
    /** Control/decoder operation: a retired decoder cannot publish into the new ring. */
    synchronized long flush(){long next=epoch.incrementAndGet();if(next<0)throw new IllegalStateException("PCM generation exhausted");buffer=new Buffer(capacity,next,mixer.frame);return next;}
    void close(){mixer.remove(this);}

    /** -1 means no stable position snapshot is retained; never invent a seek to zero. */
    long positionFrames(long presentedFrame){
      Buffer b=buffer;long version=b.version.get();if((version&1)!=0)return -1;
      int count=b.markerCount,next=b.markerNext,oldest=(next-count+MARKERS)%MARKERS;
      if(count>0&&b.mediaStart[oldest]>0&&presentedFrame<b.outputStart[oldest])return -1;
      long position=0;
      for(int i=0;i<count;i++){int at=(oldest+i)%MARKERS;if(presentedFrame<b.outputStart[at])break;int elapsed=(int)Math.min(b.markerFrames[at],presentedFrame-b.outputStart[at]);position=b.mediaStart[at]+elapsed;if(elapsed<b.markerFrames[at])break;}
      return buffer==b&&b.version.get()==version?position:-1;
    }
    boolean pending(long presentedFrame){Buffer b=buffer;return b.read!=b.write||presentedFrame<b.lastOutputFrame;}
    boolean drained(long presentedFrame){Buffer b=buffer;return b.ended&&b.read==b.write&&presentedFrame>=b.lastOutputFrame;}
  }
  private static final class Buffer {
    final int capacity;
    final long generation;
    final short[] pcm;
    volatile long read,write,lastOutputFrame;
    volatile boolean ended;
    long consumed;
    int markerCount,markerNext;
    final AtomicLong version=new AtomicLong();
    final long[] outputStart=new long[MARKERS],mediaStart=new long[MARKERS];
    final int[] markerFrames=new int[MARKERS];
    Buffer(int capacity,long generation,long frame){this.capacity=capacity;this.generation=generation;lastOutputFrame=frame;pcm=new short[capacity*2];}
    void marker(long output,long media,int count){version.incrementAndGet();outputStart[markerNext]=output;mediaStart[markerNext]=media;markerFrames[markerNext]=count;markerNext=(markerNext+1)%MARKERS;markerCount=Math.min(MARKERS,markerCount+1);lastOutputFrame=output+count;version.incrementAndGet();}
  }
}
