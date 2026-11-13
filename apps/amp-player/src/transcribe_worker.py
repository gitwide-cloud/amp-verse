"""Optional local ASR worker: bounded audio chunks, no audio uploads."""
import argparse,json,os,subprocess,time,sys
from pathlib import Path
MODEL="Systran/faster-whisper-base"
REVISION="ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66"
def emit(**event):print(json.dumps(event,ensure_ascii=False),flush=True)
def checkpoint():
    marker=os.environ.get("AMP_PREPARATION_PAUSE_FILE")
    while marker:
        try:
            parts=Path(marker).read_text().split();remaining=int(parts[0])-time.time()*1000
            if not 0<remaining<=10000:return False
            if len(parts)>1 and parts[1]=="slow":time.sleep(.75);return True
            emit(type="status",text="Paused for playback");time.sleep(.25)
        except (OSError,ValueError,IndexError):return False
    return False
def hidden():return {"creationflags":subprocess.CREATE_NO_WINDOW} if os.name=="nt" else {}
def run(args,**kwargs):return subprocess.run(args,**kwargs,**hidden())
def transcribe(source,destination,models,language,device,ffmpeg,ffprobe):
    checkpoint()
    probe=run([ffprobe,"-v","error","-show_entries","format=duration","-of","default=noprint_wrappers=1:nokey=1",str(source)],capture_output=True,text=True,check=False)
    if probe.returncode:raise RuntimeError("Cannot decode this recording: "+probe.stderr[-500:])
    duration=float(probe.stdout.strip())
    if not 0<duration<=86400:raise RuntimeError("Choose a recording shorter than 24 hours, or split it into chapters first")
    import numpy as np
    from huggingface_hub import snapshot_download
    from faster_whisper import WhisperModel
    model_dir=Path(models)/"base"
    if not all((model_dir/name).is_file() for name in ("model.bin","config.json","tokenizer.json")):
        emit(type="status",text="Downloading the optional base speech model (first use only)")
        snapshot_download(MODEL,revision=REVISION,local_dir=str(model_dir),allow_patterns=["config.json","model.bin","tokenizer.json","vocabulary.*","preprocessor_config.json"])
    handles=[]
    for folder in os.environ.get("AMP_TRANSCRIBE_CUDA_LIB","").split(os.pathsep):
        if folder and Path(folder).is_dir() and hasattr(os,"add_dll_directory"):handles.append(os.add_dll_directory(folder))
    chosen="cpu"
    if device=="auto" and not checkpoint():
        try:
            import ctranslate2
            if ctranslate2.get_cuda_device_count()>0:chosen="cuda"
        except Exception:pass
    def model(which):return WhisperModel(str(model_dir),device=which,compute_type="float16" if which=="cuda" else "int8",cpu_threads=2,num_workers=1,local_files_only=True)
    try:recognizer=model(chosen)
    except Exception:
        if chosen=="cpu":raise
        emit(type="status",text="GPU runtime unavailable; using CPU");chosen="cpu";recognizer=model(chosen)
    emit(type="status",text="Transcribing locally on "+chosen.upper())
    log=Path(destination).with_suffix(".decoder.log")
    segments=[];text_bytes=0;offset=0.;selected_language=language or None
    with log.open("wb") as errors:
        decoder=subprocess.Popen([ffmpeg,"-nostdin","-v","error","-threads","1","-i",str(source),"-vn","-ac","1","-ar","16000","-f","s16le","pipe:1"],stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=errors,**hidden())
        try:
            while True:
                checkpoint();chunk=decoder.stdout.read(16000*2*30)
                if not chunk:break
                if len(chunk)%2:raise RuntimeError("Incomplete decoded audio")
                audio=np.frombuffer(chunk,dtype="<i2").astype(np.float32)/32768.
                def recognize():return recognizer.transcribe(audio,language=selected_language,beam_size=5,vad_filter=True,condition_on_previous_text=False)
                try:parts,info=recognize();parts=list(parts)
                except Exception:
                    if chosen=="cpu":raise
                    emit(type="status",text="GPU inference unavailable; continuing on CPU");chosen="cpu";recognizer=model(chosen);parts,info=recognize();parts=list(parts)
                if parts and selected_language is None:selected_language=info.language
                for part in parts:
                    checkpoint();text=part.text.strip();start=min(duration,offset+max(0.,part.start));end=min(duration,offset+part.end)
                    if text and end>start:segments.append(dict(start=start,end=end,text=text));text_bytes+=len(text.encode("utf8"))
                if len(segments)>100000 or text_bytes>8*1024*1024:raise RuntimeError("Transcript is too large; split the recording into chapters")
                offset+=len(audio)/16000.;emit(type="progress",seconds=min(offset,duration),duration=duration,segments=len(segments))
            if decoder.wait()!=0:raise RuntimeError("Audio decoding stopped: "+log.read_text(errors="replace")[-500:])
        finally:
            if decoder.poll() is None:decoder.kill();decoder.wait()
    if not segments:raise RuntimeError("No intelligible speech was found. Try the original recording if vocal isolation removed speech.")
    result=dict(version=1,source_sha256="",model="faster-whisper/base@"+REVISION,language=selected_language or "unknown",duration=duration,input="original",segments=segments)
    pending=Path(str(destination)+".pending");pending.write_text(json.dumps(result,ensure_ascii=False),encoding="utf8");os.replace(pending,destination)
def main():
    parser=argparse.ArgumentParser();parser.add_argument("source");parser.add_argument("destination");parser.add_argument("--models",required=True);parser.add_argument("--language",default="");parser.add_argument("--device",choices=["cpu","auto"],default="cpu");parser.add_argument("--ffmpeg",default="ffmpeg");parser.add_argument("--ffprobe",default="ffprobe");args=parser.parse_args()
    try:transcribe(Path(args.source),Path(args.destination),args.models,args.language,args.device,args.ffmpeg,args.ffprobe)
    except Exception as error:emit(type="error",text=str(error));print(str(error),file=sys.stderr);raise SystemExit(1)
if __name__=="__main__":main()
