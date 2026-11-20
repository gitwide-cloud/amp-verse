package cloud.gitwide.ampverse;
import java.io.*;
import java.security.*;
import java.util.*;
import java.util.function.*;
/** Writes only a new staged file; failures remove it and cannot replace existing audio. */
final class PrivateFileTransfer {
 static final class Result {final long bytes;final String sha256;Result(long b,String s){bytes=b;sha256=s;}}
 static Result copy(InputStream in,File target,long expected,long maximum,BooleanSupplier cancelled,LongConsumer progress)throws Exception {
  if(expected>maximum||maximum<=0)throw new IOException("Song exceeds download limit");
  if(!target.createNewFile())throw new IOException("Existing audio was retained");
  boolean complete=false;long count=0;MessageDigest digest=MessageDigest.getInstance("SHA-256");
  try{
   try(FileOutputStream out=new FileOutputStream(target)){byte[] buffer=new byte[65536];int n;while((n=in.read(buffer))!=-1){if(cancelled.getAsBoolean())throw new IOException("Download cancelled");count+=n;if(count>maximum||expected>=0&&count>expected)throw new IOException("Download length changed");out.write(buffer,0,n);digest.update(buffer,0,n);progress.accept(count);}out.getFD().sync();}
   if(cancelled.getAsBoolean()||count==0||expected>=0&&count!=expected)throw new IOException("Incomplete download");
   StringBuilder hash=new StringBuilder();for(byte b:digest.digest())hash.append(String.format(Locale.ROOT,"%02x",b&255));complete=true;return new Result(count,hash.toString());
  }finally{if(!complete)target.delete();}
 }
}
