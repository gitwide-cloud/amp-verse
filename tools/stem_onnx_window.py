"""Research-only bounded ONNX PCM worker. No downloads, checkpoints, WAVs or databases.
Only the pinned single-file StemSplitio htdemucs ONNX graph is accepted.
Not yet connected to playback. Model/driver workspace is separate from PCM budget.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import time
sys.dont_write_bytecode = True
MODEL_SHA256 = "68d0bf16428ef66e692cdff8a9ccf28f1ef3f69440d57e58605a4cc55fcc5e74"
MODEL_BYTES = 316446953
MODEL_FRAMES = 343980
MAX_FRAMES = 485100


def read_request(stream):
    line = stream.readline(1025)
    if not line:
        return None
    if len(line) > 1024 or not line.endswith(b"\n"):
        raise ValueError("Invalid bounded request header")
    h = json.loads(line)
    if not isinstance(h, dict) or h.get("version") != 1:
        raise ValueError("Unsupported request")
    frames = h.get("frames")
    if type(frames) is not int or not 1 <= frames <= MAX_FRAMES:
        raise ValueError("Window must be between one frame and eleven seconds")
    if h.get("sample_rate") != 44100 or h.get("channels") != 2:
        raise ValueError("Use stereo44.1kHz float32 PCM")
    token = h.get("request_id")
    if not isinstance(token, str) or not 1 <= len(token) <= 64 or not all(c.isascii() and (c.isalnum() or c in "-_") for c in token):
        raise ValueError("Invalid request token")
    length = frames * 8
    audio = bytearray()
    while len(audio) < length:
        chunk = stream.read(length - len(audio))
        if not chunk:
            raise ValueError("Truncated PCM")
        audio.extend(chunk)
    return h, audio


def verify_model(path, kind="scnet"):
    expected_bytes, expected_hash = ((12900277, "98228931494151762a1c4ab1ec7899a894b1f81fd4a509921a9d2f9bacc50845") if kind == "scnet" else (MODEL_BYTES, MODEL_SHA256))
    path = Path(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_size != expected_bytes:
        raise ValueError("Pinned local ONNX file required")
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1048576), b""):
            digest.update(block)
    if digest.hexdigest() != expected_hash:
        raise ValueError("Model SHA256 mismatch")
    return path


# Spectral framing follows SCNet's published contract; standard FFT/OLA math.
# Reference: audiojs/neural-separate (MIT), model card square-zero-labs/scnet.
def scnet_input(audio, np):
    reference = audio.mean(axis=1, dtype=np.float64)
    mean = float(reference.mean())
    scale = float(reference.std(ddof=1)) + 1e-8 if len(reference)>1 else 1e-8
    normalized = (audio.astype(np.float64)-mean)/scale
    original_frames = len(audio)
    length = 485100
    mode = "reflect" if original_frames > length//2 else "constant"
    normalized = np.pad(normalized, ((0,length-original_frames),(0,0)), mode=mode)
    normalized = np.pad(normalized, ((0,486400-length),(0,0)))
    centered = np.pad(normalized, ((2048,2048),(0,0)), mode="reflect")
    spec = np.empty((1,4,2049,476), dtype=np.float32)
    for channel in range(2):
        frames = np.lib.stride_tricks.sliding_window_view(centered[:,channel],4096)[::1024]
        z = np.fft.rfft(frames, axis=1, norm="ortho").T
        spec[0,channel*2] = z.real
        spec[0,channel*2+1] = z.imag
    return spec, scale, mean


def scnet_inverse(spec, frames, scale, np):
    # spec has four complex stereo sources, [1,16,F,T]. Do not save it.
    planes = spec[0].reshape(4,2,2,2049,476)
    length=475*1024+4096
    weight=np.zeros(length,dtype=np.float64)
    for t in range(476):weight[t*1024:t*1024+4096]+=1.
    result=np.empty((4,2,frames),dtype=np.float32)
    for source in range(4):
        for channel in range(2):
            z=planes[source,channel,0].astype(np.float64)+1j*planes[source,channel,1]
            wave=np.fft.irfft(z.T*64.,n=4096,axis=1)
            out=np.zeros(length,dtype=np.float64)
            for t in range(476):out[t*1024:t*1024+4096]+=wave[t]
            result[source,channel]=(out[2048:2048+frames]/weight[2048:2048+frames])*scale
    return result


def constrain_memory(mebibytes):
    limit = mebibytes * 1024 * 1024
    if sys.platform == "win32":
        import ctypes
        from ctypes import wintypes as wt
        class Basic(ctypes.Structure):
            _fields_ = [("PerProcessUserTimeLimit", ctypes.c_longlong), ("PerJobUserTimeLimit", ctypes.c_longlong), ("LimitFlags", wt.DWORD), ("MinimumWorkingSetSize", ctypes.c_size_t), ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", wt.DWORD), ("Affinity", ctypes.c_size_t), ("PriorityClass", wt.DWORD), ("SchedulingClass", wt.DWORD)]
        class Io(ctypes.Structure):
            _fields_ = [(name, ctypes.c_ulonglong) for name in ("ReadOperationCount", "WriteOperationCount", "OtherOperationCount", "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]
        class Extended(ctypes.Structure):
            _fields_ = [("BasicLimitInformation", Basic), ("IoInfo", Io), ("ProcessMemoryLimit", ctypes.c_size_t), ("JobMemoryLimit", ctypes.c_size_t), ("PeakProcessMemoryUsed", ctypes.c_size_t), ("PeakJobMemoryUsed", ctypes.c_size_t)]
        k = ctypes.windll.kernel32
        k.CreateJobObjectW.argtypes = [ctypes.c_void_p, wt.LPCWSTR]
        k.CreateJobObjectW.restype = wt.HANDLE
        k.SetInformationJobObject.argtypes = [wt.HANDLE, ctypes.c_int, ctypes.c_void_p, wt.DWORD]
        k.AssignProcessToJobObject.argtypes = [wt.HANDLE, wt.HANDLE]
        k.GetCurrentProcess.restype = wt.HANDLE
        job = k.CreateJobObjectW(None, None)
        if not job:
            raise RuntimeError("Cannot create worker memory-limit job")
        info = Extended()
        info.BasicLimitInformation.LimitFlags = 0x200
        info.JobMemoryLimit = limit
        if not k.SetInformationJobObject(job, 9, ctypes.byref(info), ctypes.sizeof(info)) or not k.AssignProcessToJobObject(job, k.GetCurrentProcess()):
            raise RuntimeError("Cannot enforce worker job memory limit; model will not load")
        return job, "windows-job-commit-bytes"
    if sys.platform.startswith("linux"):
        import resource
        soft, hard = resource.getrlimit(resource.RLIMIT_AS)
        cap = limit if hard == resource.RLIM_INFINITY else min(limit, hard)
        resource.setrlimit(resource.RLIMIT_AS, (cap, hard))
        return None, "linux-address-space-bytes"
    raise RuntimeError("Worker memory limits are not implemented on this platform")


def peak_rss_bytes():
    if sys.platform == "win32":
        import ctypes
        from ctypes import wintypes as wt
        class Counters(ctypes.Structure):
            _fields_ = [("cb", wt.DWORD), ("PageFaultCount", wt.DWORD)] + [(name, ctypes.c_size_t) for name in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage")]
        values = Counters()
        values.cb = ctypes.sizeof(values)
        current = ctypes.windll.kernel32.GetCurrentProcess
        current.restype = wt.HANDLE
        get = ctypes.windll.psapi.GetProcessMemoryInfo
        get.argtypes = [wt.HANDLE, ctypes.POINTER(Counters), wt.DWORD]
        if not get(current(), ctypes.byref(values), values.cb):
            raise RuntimeError("Worker memory telemetry unavailable")
        return values.PeakWorkingSetSize
    import resource
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * 1024


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True)
    parser.add_argument("--model-kind", choices=("scnet","htdemucs"), default="scnet")
    parser.add_argument("--provider", choices=("cpu", "dml", "cuda"), default="cpu")
    parser.add_argument("--device-id", type=int, default=0)
    parser.add_argument("--requests", type=int, choices=range(0,257), default=1)
    parser.add_argument("--memory-mib", type=int, default=6144)
    parser.add_argument("--cuda-dll-dir")
    args = parser.parse_args()
    if not 512 <= args.memory_mib <= 6144:
        parser.error("memory-mib must be512..6144")
    memory_job, memory_limit_kind = constrain_memory(args.memory_mib)
    if args.device_id < 0:
        parser.error("device-id must be nonnegative")
    request = read_request(sys.stdin.buffer)
    if request is None:
        return
    header, pcm = request
    model = verify_model(args.model, args.model_kind)
    import numpy as np
    import onnxruntime as ort
    audio = np.frombuffer(pcm, dtype="<f4").reshape(header["frames"], 2)
    if not np.isfinite(audio).all():
        raise ValueError("Non-finite source audio")
    options = ort.SessionOptions()
    options.intra_op_num_threads = 2
    options.inter_op_num_threads = 1
    options.enable_cpu_mem_arena = False
    options.enable_mem_pattern = False
    begin = time.perf_counter()
    options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    providers = ["CPUExecutionProvider"]
    if args.provider == "dml":
        if "DmlExecutionProvider" not in ort.get_available_providers():
            raise RuntimeError("Requested DirectML provider unavailable")
        providers = [("DmlExecutionProvider", {"device_id": str(args.device_id)}), "CPUExecutionProvider"]
    if args.provider == "cuda":
        if "CUDAExecutionProvider" not in ort.get_available_providers():
            raise RuntimeError("Requested CUDA provider unavailable")
        if args.cuda_dll_dir:
            ort.preload_dlls(directory=args.cuda_dll_dir)
        providers = [("CUDAExecutionProvider", {"device_id": str(args.device_id), "gpu_mem_limit": str(3 * 1024**3), "cudnn_conv_algo_search": "HEURISTIC", "cudnn_conv_use_max_workspace": "0"}), "CPUExecutionProvider"]
    session = ort.InferenceSession(str(model), sess_options=options, providers=providers)
    wanted = {"cpu": "CPUExecutionProvider", "dml": "DmlExecutionProvider", "cuda": "CUDAExecutionProvider"}[args.provider]
    if session.get_providers()[0] != wanted:
        raise RuntimeError("Requested provider was not activated")
    session.disable_fallback()
    inputs, outputs = session.get_inputs(), session.get_outputs()
    input_name, output_name, shape = (("mix_spec","stems_spec",[1,4,2049,476]) if args.model_kind == "scnet" else ("mix","stems",[1,2,MODEL_FRAMES]))
    if len(inputs)!=1 or inputs[0].name!=input_name or inputs[0].shape!=shape or inputs[0].type!="tensor(float)":
        raise ValueError("Unexpected model input contract")
    if len(outputs)!=1 or outputs[0].name!=output_name:
        raise ValueError("Unexpected model output contract")
    loaded = time.perf_counter()
    index = 0
    while args.requests == 0 or index < args.requests:
        if index:
            request = read_request(sys.stdin.buffer)
            if request is None:
                break
            header, pcm = request
            audio = np.frombuffer(pcm, dtype="<f4").reshape(header["frames"], 2)
            if not np.isfinite(audio).all():
                raise ValueError("Non-finite source audio")
        started = time.perf_counter()
        if args.model_kind == "scnet":
            tensor, scale, _ = scnet_input(audio,np)
            result=session.run([output_name],{input_name:tensor})[0]
            if result.shape!=(1,16,2049,476) or not np.isfinite(result).all():
                raise ValueError("Invalid SCNet spectral output")
            parts=scnet_inverse(result,header["frames"],scale,np)
        else:
            tensor=np.zeros((1,2,MODEL_FRAMES),dtype=np.float32)
            tensor[0,:,:header["frames"]]=audio.T
            result=session.run([output_name],{input_name:tensor})[0]
            if result.shape!=(1,4,2,MODEL_FRAMES) or not np.isfinite(result).all():
                raise ValueError("Invalid bounded model output")
            parts=result[0,:,:,:header["frames"]]
        # The graph emits drums,bass,other,vocals. Keep explicit wire order.
        ordered = [parts[3], parts[0], parts[1]]
        ordered.append(audio.T - ordered[0] - ordered[1] - ordered[2])
        if not all(np.isfinite(part).all() for part in ordered):
            raise ValueError("Invalid rescaled output")
        ended = time.perf_counter()
        response = {"version": 1, "request_id": header["request_id"], "frames": header["frames"], "sample_rate": 44100, "channels": 2, "parts": ["vocals", "drums", "bass", "other"], "model_kind": args.model_kind, "provider": session.get_providers()[0], "configured_providers": session.get_providers(), "peak_worker_rss_bytes": peak_rss_bytes(), "hard_model_memory_limit_enforced": True, "memory_limit_mib": args.memory_mib, "memory_limit_kind": memory_limit_kind, "load_seconds": (loaded-begin if index == 0 else 0.), "inference_seconds": ended-started, "real_time_factor": (ended-started)/(header["frames"]/44100), "persistent_audio_bytes": 0}
        sys.stdout.buffer.write(json.dumps(response).encode()+b"\n")
        for part in ordered:
            sys.stdout.buffer.write(part.T.astype("<f4", copy=False).tobytes())
        sys.stdout.buffer.flush()
        index += 1



if __name__ == "__main__":
    main()
