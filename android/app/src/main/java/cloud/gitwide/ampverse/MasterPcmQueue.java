package cloud.gitwide.ampverse;

/** Bounded SPSC post-master ring: render producer, encoder consumer, no monitors. */
final class MasterPcmQueue {
  private final short[] pcm;
  private final int capacity;
  private volatile long read,write,rejected;
  private volatile boolean closed;
  private long nextMasterFrame=-1;
  MasterPcmQueue(int capacity){if(capacity<1||capacity>OwnedPcmMixer.RATE)throw new IllegalArgumentException("Export queue bounded to one second");this.capacity=capacity;pcm=new short[capacity*2];}
  boolean offer(short[] input,int frames,long masterFrame){
    long at=write,consumed=read;
    if(closed||frames<0||(long)frames*2>input.length||frames>capacity-(at-consumed)||(nextMasterFrame>=0&&masterFrame!=nextMasterFrame))return false;
    for(int i=0;i<frames;i++){int target=(int)((at+i)%capacity)*2;pcm[target]=input[i*2];pcm[target+1]=input[i*2+1];}
    nextMasterFrame=masterFrame+frames;write=at+frames;return true;
  }
  int read(short[] output,int maxFrames){
    if(maxFrames<0||(long)maxFrames*2>output.length)throw new IllegalArgumentException("Invalid PCM read range");
    if(closed)return 0;long at=read;int frames=(int)Math.min(maxFrames,write-at);
    for(int i=0;i<frames;i++){int source=(int)((at+i)%capacity)*2;output[i*2]=pcm[source];output[i*2+1]=pcm[source+1];}
    read=at+frames;return frames;
  }
  int frames(){long consumed=read;return closed?0:(int)Math.max(0,Math.min(capacity,write-consumed));}
  void close(){closed=true;}
  void reject(int frames){rejected+=frames;}
  long rejectedFrames(){return rejected;}
}
