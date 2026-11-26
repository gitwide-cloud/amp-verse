import io,json,unittest,subprocess,sys
from pathlib import Path
import stem_onnx_window as worker

class ProtocolTests(unittest.TestCase):
    def request(self, **kwargs):
        h={"version":1,"request_id":"test","frames":2,"sample_rate":44100,"channels":2}
        h.update(kwargs)
        return json.dumps(h).encode()+b"\n"+bytes(16)
    def test_empty_stream_is_clean_eof(self):
        self.assertIsNone(worker.read_request(io.BytesIO(b"")))
    def test_valid_bounded_pcm(self):
        h,pcm=worker.read_request(io.BytesIO(self.request()))
        self.assertEqual(h["frames"],2)
        self.assertEqual(len(pcm),16)
    def test_huge_window_rejected_before_pcm_read(self):
        with self.assertRaises(ValueError):worker.read_request(io.BytesIO(self.request(frames=10**12)))
    def test_wrong_rate_or_channels_rejected(self):
        for args in ({"sample_rate":48000},{"channels":3},{"frames":True},{"frames":0},{"request_id":"bad/path"}):
            with self.subTest(args=args),self.assertRaises(ValueError):worker.read_request(io.BytesIO(self.request(**args)))
    def test_truncated_pcm_rejected(self):
        with self.assertRaises(ValueError):worker.read_request(io.BytesIO(self.request()[:-1]))
    def test_header_bound(self):
        with self.assertRaises(ValueError):worker.read_request(io.BytesIO(b"x"*1025))
    def test_os_limit_stops_excess_allocation(self):
        script = "import stem_onnx_window as w;w.constrain_memory(128)\ntry: b=bytearray(256*1024*1024)\nexcept MemoryError: raise SystemExit(0)\nraise SystemExit(1)"
        result = subprocess.run([sys.executable, "-B", "-c", script], cwd=Path(worker.__file__).parent, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
    def test_arbitrary_or_missing_model_refused(self):
        with self.assertRaises(ValueError):worker.verify_model("not-a-model.onnx")

if __name__=="__main__":unittest.main()
