"""Focused, offline worker tests; no models or audio uploads required."""
import importlib.util,io,json,sys,tempfile,types,unittest
import numpy
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('speech_worker',Path(__file__).parents[1]/'apps/amp-player/src/transcribe_worker.py');worker=importlib.util.module_from_spec(spec);spec.loader.exec_module(worker)
class Decoder:
    def __init__(self,*a,**k):self.stdout=io.BytesIO(bytes(16000*2*65));self.code=None
    def wait(self):self.code=0;return 0
    def poll(self):return self.code
    def kill(self):self.code=-1
class Model:
    empty=False
    def __init__(self,*a,**k):pass
    def transcribe(self,*a,**k):return iter([] if self.empty else [types.SimpleNamespace(start=1.,end=2.,text=' Spoken words ')]),types.SimpleNamespace(language='en')
class Tests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name);(self.root/'base').mkdir();[(self.root/'base'/name).touch() for name in ('model.bin','config.json','tokenizer.json')];self.output=self.root/'transcript.json'
    def tearDown(self):self.temp.cleanup();Model.empty=False
    def run_worker(self,probe=0):
        with patch.dict(sys.modules,{'faster_whisper':types.SimpleNamespace(WhisperModel=Model),'huggingface_hub':types.SimpleNamespace(snapshot_download=lambda *a,**k:self.fail('Unexpected model download'))}),patch.object(worker,'run',return_value=types.SimpleNamespace(returncode=probe,stdout='65',stderr='bad audio')),patch.object(worker.subprocess,'Popen',Decoder),patch.object(worker,'emit'):
            worker.transcribe(self.root/'source.wav',self.output,self.root,'','cpu','ffmpeg','ffprobe')
    def test_chunk_offsets_and_atomic_completed_output(self):
        self.run_worker();doc=json.loads(self.output.read_text());self.assertEqual([s['start'] for s in doc['segments']],[1.,31.,61.]);self.assertFalse(Path(str(self.output)+'.pending').exists())
    def test_no_speech_preserves_previous_transcript(self):
        self.output.write_text('previous');Model.empty=True
        with self.assertRaisesRegex(RuntimeError,'No intelligible speech'):self.run_worker()
        self.assertEqual(self.output.read_text(),'previous')
    def test_bad_audio_preserves_previous_transcript(self):
        self.output.write_text('previous')
        with self.assertRaisesRegex(RuntimeError,'Cannot decode'):self.run_worker(1)
        self.assertEqual(self.output.read_text(),'previous')
    def test_playback_slow_gate(self):
        marker=self.root/'gate';marker.write_text('1005000 slow')
        with patch.dict(worker.os.environ,{'AMP_PREPARATION_PAUSE_FILE':str(marker)}),patch.object(worker.time,'time',return_value=1000),patch.object(worker.time,'sleep') as sleep:
            self.assertTrue(worker.checkpoint());sleep.assert_called_once_with(.75)
    def test_stale_playback_marker_ignored(self):
        marker=self.root/'gate';marker.write_text('1 pause')
        with patch.dict(worker.os.environ,{'AMP_PREPARATION_PAUSE_FILE':str(marker)}):self.assertFalse(worker.checkpoint())
if __name__=='__main__':unittest.main()
