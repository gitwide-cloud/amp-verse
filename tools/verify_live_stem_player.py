"""Exercise the real player's live pipeline with an original test fixture, never stem files."""
import argparse
import array
import json
import os
from pathlib import Path
import subprocess
import tempfile
import wave

from test_live_stems import fixture, validate_module


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--module', type=Path, required=True)
    args = parser.parse_args()
    module = args.module.resolve()
    module, manifest = validate_module(module)
    env = os.environ.copy()
    env['AMP_ONNX_WORKER'] = str(module / manifest['relative_paths']['worker'])
    env['AMP_ONNX_MODEL'] = str(module / manifest['relative_paths']['model'])
    env['ORT_DYLIB_PATH'] = str(module / manifest['relative_paths']['runtime'])
    # This is original synthetic test input, not a saved separated output.
    floats = fixture()
    integers = array.array('h', (round(max(-1, min(1, value)) * 32767) for value in floats))
    with tempfile.TemporaryDirectory(prefix='amp-live-original-fixture-') as directory:
        source = Path(directory) / 'original-synthetic-input.wav'
        with wave.open(str(source), 'wb') as output:
            output.setparams((2, 2, 44100, 0, 'NONE', 'not compressed'))
            output.writeframes(integers.tobytes() * 6)
        env['AMP_STEM_TEST_SOURCE'] = str(source)
        subprocess.run([
            'cargo', 'test', '-p', 'amp-player', '--bin', 'amp-player',
            'onnx_stem_stream::live_acceptance::real_model_source_pipeline_preserves_mix_across_windows',
            '--locked', '--', '--ignored', '--exact', '--test-threads=1', '--nocapture',
        ], env=env, check=True)
        assert sorted(p.name for p in Path(directory).iterdir()) == [source.name]
    print(json.dumps({'full_player_live_pipeline': 'passed', 'saved_stem_audio_files': 0}))


if __name__ == '__main__':
    main()
