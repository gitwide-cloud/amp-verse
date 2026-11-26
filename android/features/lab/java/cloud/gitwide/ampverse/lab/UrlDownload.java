package cloud.gitwide.ampverse.lab;

import android.content.Context;
import android.media.MediaMetadataRetriever;
import android.system.Os;
import android.system.OsConstants;
import com.arthenica.ffmpegkit.FFmpegKit;
import com.arthenica.ffmpegkit.FFmpegSession;
import com.arthenica.ffmpegkit.ReturnCode;
import com.yausername.youtubedl_android.YoutubeDL;
import org.json.JSONObject;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.*;
import java.util.concurrent.*;

/** Lab-only importer. Each worker has an isolated process group and temporary directory. */
final class UrlDownload {
    private static File bundledExtractor;
    private static synchronized File extractor(Context context)throws Exception{
        if(bundledExtractor!=null&&bundledExtractor.isFile())return bundledExtractor;
        File directory=new File(context.getNoBackupFilesDir(),"amp-lab-tools");if(!directory.isDirectory()&&!directory.mkdirs())throw new IOException("Cannot prepare the bundled download tools");
        File temporary=File.createTempFile("extractor-",".pending",directory);
        try{java.security.MessageDigest digest=java.security.MessageDigest.getInstance("SHA-256");
            try(InputStream in=context.getResources().openRawResource(com.yausername.youtubedl_android.R.raw.ytdlp);FileOutputStream out=new FileOutputStream(temporary)){byte[] buffer=new byte[65536];int n;long count=0;while((n=in.read(buffer))!=-1){count+=n;if(count>32*1024*1024)throw new IOException("Bundled extractor exceeds its size limit");digest.update(buffer,0,n);out.write(buffer,0,n);}out.getFD().sync();}
            StringBuilder hash=new StringBuilder();for(byte b:digest.digest())hash.append(String.format(Locale.ROOT,"%02x",b));
            File target=new File(directory,"yt-dlp-"+hash+".pyz");
            // Versioned immutable paths also avoid changing code under an active job.
            Files.move(temporary.toPath(),target.toPath(),java.nio.file.StandardCopyOption.ATOMIC_MOVE,java.nio.file.StandardCopyOption.REPLACE_EXISTING);
            bundledExtractor=target;return target;
        }finally{temporary.delete();}
    }
    interface Listener { void progress(String value); void ready(File source); void failed(String message); default void inspected(JSONObject info){} }
    private final Context context;
    private final Listener listener;
    private volatile boolean cancelled;
    private volatile Process process;
    private volatile File pidFile;
    private volatile long conversion = -1;
    UrlDownload(Context context, Listener listener) { this.context=context.getApplicationContext();this.listener=listener; }
    void start(String url) { new Thread(()->run(url,false), "AMP Lab URL import").start(); }
    void inspect(String url) { new Thread(()->run(url,true), "AMP Lab URL inspection").start(); }
    void cancel() {
        cancelled=true;
        if(conversion>=0) FFmpegKit.cancel(conversion);
        killGroup();
    }
    boolean cancelled(){return cancelled;}
    static File publish(File source,File directory,String title,UUID identity)throws IOException{
        File completed=new File(directory,safeTitle(title)+"-"+identity+".mp3");
        try(RandomAccessFile durable=new RandomAccessFile(source,"rw")){durable.getFD().sync();}
        // Both names live in app-private storage on the same filesystem. link(2)
        // claims the final name atomically and cannot overwrite another download.
        Files.createLink(completed.toPath(),source.toPath());
        Files.delete(source.toPath());return completed;
    }
    private synchronized void killGroup() {
        File pid=pidFile;
        if(pid!=null) try {
            String value=new String(Files.readAllBytes(pid.toPath()),StandardCharsets.US_ASCII).trim();
            if(value.matches("[0-9]{1,10}")) { int group=Integer.parseInt(value);if(group>1)Os.kill(-group,OsConstants.SIGKILL); }
        } catch(Exception ignored) { }
        Process active=process;if(active!=null)active.destroyForcibly();
    }
    private void check() throws IOException { if(cancelled)throw new IOException("Cancelled"); }
    private String execute(File work,List<String> options,boolean metadata)throws Exception {
        check();
        File base=new File(context.getNoBackupFilesDir(),"youtubedl-android");
        File pythonHome=new File(base,"packages/python/usr");
        File bin=new File(context.getApplicationInfo().nativeLibraryDir);
        File python=new File(bin,"libpython.so"),quickjs=new File(bin,"libqjs.so"),extractor=extractor(context);
        if(!python.isFile()||!quickjs.isFile()||!extractor.isFile())throw new IOException("Download tools are missing. Update AMP Lab in Add-ons and retry.");
        pidFile=new File(work,"worker.pid");Files.deleteIfExists(pidFile.toPath());
        // Own the process group instead of the wrapper's shell-based cancellation.
        String entry="import os,sys,runpy; os.setsid(); open(sys.argv[1],'w').write(str(os.getpid())); sys.argv=sys.argv[2:]; runpy.run_path(sys.argv[0],run_name='__main__')";
        List<String> args=new ArrayList<>(Arrays.asList(python.toString(),"-c",entry,pidFile.toString(),extractor.toString(),"--ignore-config","--no-playlist","--no-warnings","--socket-timeout","20","--retries","2","--no-cache-dir","--js-runtimes","quickjs:"+quickjs));
        args.addAll(options);
        ProcessBuilder builder=new ProcessBuilder(args).directory(work).redirectErrorStream(true);
        Map<String,String> env=builder.environment();env.put("PYTHONHOME",pythonHome.toString());env.put("HOME",pythonHome.toString());env.put("LD_LIBRARY_PATH",new File(pythonHome,"lib").toString());env.put("SSL_CERT_FILE",new File(pythonHome,"etc/tls/cert.pem").toString());env.put("TMPDIR",work.toString());env.put("PATH",System.getenv("PATH")+":"+bin);
        process=builder.start();
        // Cancellation can race startup before the PID file is written.
        ScheduledExecutorService watchdog=Executors.newSingleThreadScheduledExecutor();
        long deadline=System.nanoTime()+TimeUnit.MINUTES.toNanos(metadata?2:60);
        watchdog.scheduleAtFixedRate(()->{if(cancelled||System.nanoTime()>deadline){cancelled=true;killGroup();}},100,100,TimeUnit.MILLISECONDS);
        StringBuilder output=new StringBuilder();
        try(BufferedReader reader=new BufferedReader(new InputStreamReader(process.getInputStream(),StandardCharsets.UTF_8))){
            String line;while((line=reader.readLine())!=null){
                check();
                if(metadata){if(output.length()+line.length()>2*1024*1024)throw new IOException("Video information is too large");output.append(line).append('\n');}
                else {if(line.startsWith("[download]"))listener.progress(line.replace("[download]", "Downloading").trim());if(output.length()>8192)output.delete(0,4096);output.append(line).append('\n');}
            }
            int result=process.waitFor();check();
            if(result!=0)throw new IOException(readable(output.toString()));
            return output.toString();
        }finally {killGroup();watchdog.shutdownNow();process=null;}
    }
    private static String readable(String error) {
        String text=error.toLowerCase(Locale.ROOT);
        if(text.contains("private")||text.contains("sign in")||text.contains("members-only")||text.contains("age-restricted"))return "This video requires access or sign-in. AMP Lab cannot download it.";
        if(text.contains("unavailable")||text.contains("removed"))return "This video is unavailable.";
        if(text.contains("space")||text.contains("permission denied"))return "Cannot write the download. Check storage space and retry.";
        if(text.contains("timed out")||text.contains("network")||text.contains("resolve"))return "Network connection failed. Check your connection and retry.";
        return "YouTube download failed. Update AMP Lab and retry. "+error.substring(Math.max(0,error.length()-400)).trim();
    }
    static String safeTitle(String input){String title=input.replaceAll("[\\\\/:*?\"<>|\\p{Cntrl}]","_").trim();if(title.codePointCount(0,title.length())>100)title=title.substring(0,title.offsetByCodePoints(0,100));while(title.getBytes(StandardCharsets.UTF_8).length>160)title=title.substring(0,title.offsetByCodePoints(title.length(),-1));title=title.replaceAll("[. ]+$","");if(title.isEmpty())title="YouTube audio";if(title.matches("(?i)(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\\..*)?"))title="_"+title;return title;}
    private void run(String url,boolean inspectionOnly) {
        File work=null;
        try {
            String video=YouTubeUrl.normalize(url);check();listener.progress("Preparing download tools…");
            YoutubeDL.INSTANCE.init(context);check();
            File downloads=new File(context.getFilesDir(),"AMP Lab/downloads");
            if(!downloads.isDirectory()&&!downloads.mkdirs())throw new IOException("Cannot create AMP Lab downloads folder");
            work=Files.createTempDirectory(downloads.toPath(),".pending-").toFile();
            listener.progress("Reading video information…");
            JSONObject info=new JSONObject(execute(work,Arrays.asList("--dump-single-json","--skip-download","--",video),true));
            if(info.optBoolean("is_live")||info.has("entries"))throw new IOException("Choose one completed video, not a live stream or playlist.");
            double duration=info.optDouble("duration",0);if(!Double.isFinite(duration)||duration<=0||duration>14400)throw new IOException("Choose audio up to four hours long.");
            if(inspectionOnly){check();listener.inspected(info);return;}
            check();listener.progress("Downloading audio…");
            execute(work,Arrays.asList("-f","bestaudio/best","--newline","--no-overwrites","--max-filesize","512M","-o",new File(work,"source.%(ext)s").toString(),"--",video),false);
            check();File[] sources=work.listFiles((dir,name)->name.startsWith("source.")&&!name.endsWith(".part")&&!name.endsWith(".ytdl"));
            if(sources==null||sources.length!=1||sources[0].length()==0)throw new IOException("No complete audio file was downloaded");
            File mp3=new File(work,"converted.mp3");listener.progress("Converting to MP3 · 256 kbps…");
            FFmpegSession session=FFmpegKit.executeWithArgumentsAsync(new String[]{"-nostdin","-v","error","-i",sources[0].toString(),"-vn","-map","0:a:0","-c:a","libmp3lame","-b:a","256k",mp3.toString()},null);
            conversion=session.getSessionId();
            if(cancelled)FFmpegKit.cancel(conversion);
            // FFmpegKit completes on its own executor; waiting never blocks the UI.
            while(session.getState()==com.arthenica.ffmpegkit.SessionState.CREATED||session.getState()==com.arthenica.ffmpegkit.SessionState.RUNNING){Thread.sleep(100);if(cancelled)FFmpegKit.cancel(conversion);}
            check();if(!ReturnCode.isSuccess(session.getReturnCode()))throw new IOException("MP3 conversion failed. The original edit was retained.");
            MediaMetadataRetriever verify=new MediaMetadataRetriever();try{verify.setDataSource(mp3.toString());String length=verify.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION);if(length==null||Long.parseLong(length)<=0)throw new IOException("Converted audio could not be verified");}finally{verify.release();}
            check();File completed=publish(mp3,downloads,info.optString("title","YouTube audio"),UUID.randomUUID());
            if(cancelled){listener.failed("Download cancelled; the completed MP3 was kept in Downloaded audio.");return;}
            listener.ready(completed);
        }catch(Exception error){listener.failed(cancelled?"Download cancelled. Your edit and completed downloads were kept.":readable(error.getMessage()==null?error.toString():error.getMessage()));}
        finally{conversion=-1;killGroup();if(work!=null)cleanup(work);}
    }
    private static void cleanup(File directory){File[] entries=directory.listFiles();if(entries!=null)for(File file:entries){if(file.isDirectory())cleanup(file);else file.delete();}directory.delete();}
}
