"""Verify real live playback and opt-in compressed playlist preparation; temporary test audio is cleaned up."""
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
    parser.add_argument('--six-decks', action='store_true',
                        help='Run hardware qualification: six real-model decks at 116%% for 90 seconds, then five survivors for 8 seconds. Requires a qualified CUDA GPU; CPU runs do not qualify.')
    parser.add_argument('--cuda-module', type=Path,
                        help='Pinned CUDA runtime bundle, required only with --six-decks.')
    parser.add_argument('--cuda-manifest-sha256',
                        help='Expected SHA-256 of the CUDA module manifest; required with --six-decks.')
    args = parser.parse_args()
    if args.six_decks and (args.cuda_module is None or not args.cuda_manifest_sha256):
        parser.error('--six-decks requires --cuda-module and --cuda-manifest-sha256')
    if not args.six_decks and (args.cuda_module is not None or args.cuda_manifest_sha256):
        parser.error('CUDA bundle options are used only with --six-decks')
    module = args.module.resolve()
    module, manifest = validate_module(module)
    env = os.environ.copy()
    env['AMP_ONNX_WORKER'] = str(module / manifest['relative_paths']['worker'])
    env['AMP_ONNX_MODEL'] = str(module / manifest['relative_paths']['model'])
    env['ORT_DYLIB_PATH'] = str(module / manifest['relative_paths']['runtime'])
    env['AMP_ONNX_RUNTIME'] = env['ORT_DYLIB_PATH']
    env['AMP_ONNX_PROVIDER'] = 'cpu'
    env.pop('AMP_ONNX_CUDA_DLL_DIR', None)
    env.pop('AMP_STEM_TEST_QUALIFIED_GPU', None)
    gpu_env = None
    if args.six_decks:
        from stem_cuda_module import validate_cuda_module
        cuda_root, _ = validate_cuda_module(args.cuda_module, args.cuda_manifest_sha256)
        gpu_env = env.copy()
        gpu_env['AMP_ONNX_PROVIDER'] = 'cuda'
        gpu_env['AMP_ONNX_RUNTIME'] = str(cuda_root / 'onnxruntime.dll')
        gpu_env['ORT_DYLIB_PATH'] = gpu_env['AMP_ONNX_RUNTIME']
        gpu_env['AMP_ONNX_CUDA_DLL_DIR'] = str(cuda_root / 'vendor')
        gpu_env['AMP_STEM_TEST_QUALIFIED_GPU'] = '1'
    # This is original synthetic test input, not a saved separated output.
    floats = fixture()
    integers = array.array('h', (round(max(-1, min(1, value)) * 32767) for value in floats))
    with tempfile.TemporaryDirectory(prefix='amp-live-original-fixture-') as directory:
        source = Path(directory) / 'original-synthetic-input.wav'
        with wave.open(str(source), 'wb') as output:
            output.setparams((2, 2, 44100, 0, 'NONE', 'not compressed'))
            output.writeframes(integers.tobytes() * (20 if args.six_decks else 6))
        preparation_source = Path(directory) / 'original-selected-playlist-23sec.wav'
        with wave.open(str(preparation_source), 'wb') as output:
            output.setparams((2, 2, 44100, 0, 'NONE', 'not compressed'))
            output.writeframes((integers.tobytes() * 3)[:23 * 44100 * 4])
        with wave.open(str(preparation_source), 'rb') as check:
            assert check.getnframes() == 23 * 44100
        preparation_env = env.copy()
        preparation_env['AMP_PREP_TEST_SOURCE'] = str(preparation_source)
        env['AMP_STEM_TEST_SOURCE'] = str(source)
        # Separate test processes: each exact acceptance owns a fresh shared model broker.
        tests = [
            ('dj_engine::live_stem_acceptance::six_separated_frame_controls_route_filters_and_cancel_without_model', False, env),
            ('onnx_stem_stream::live_acceptance::real_model_source_pipeline_preserves_mix_across_windows', True, env),
            ('dj_engine::live_stem_acceptance::two_native_live_stem_decks_play_filters_and_cancel_independently', True, env),
            ('prepared_stem_cache::tests::native_selected_playlist_preparation_roundtrips_compressed_stems', True, preparation_env),
        ]
        if args.six_decks:
            tests.append(('dj_engine::live_stem_acceptance::six_native_live_stem_decks_play_distinct_filters_and_cancel_independently', True, gpu_env))
        for test, ignored, test_env in tests:
            run_env = test_env.copy()
            run_env['AMP_STEM_TEST_SOURCE'] = str(source)
            command = [
                'cargo', 'test', '-p', 'amp-player', '--bin', 'amp-player', test,
                '--locked', '--',
            ]
            if ignored:
                command.append('--ignored')
            command.extend(['--exact', '--test-threads=1', '--nocapture'])
            subprocess.run(command, env=run_env, check=True)
        assert sorted(p.name for p in Path(directory).iterdir()) == sorted([source.name, preparation_source.name])
    receipt = {
        'full_player_live_pipeline': 'passed',
        'concurrent_live_decks_with_different_filters': 'passed',
        'cancel_one_other_continues': 'passed',
        'deterministic_six_deck_mixer_controls_without_model': 'passed',
        'six_live_decks_gpu_qualification': 'passed' if args.six_decks else 'not_run_requires_qualified_gpu',
        'selected_playlist_native_mp3_preparation': 'passed',
        'selected_playlist_preparation_fixture_seconds': 23,
        'selected_playlist_preparation_provider': 'cpu',
        'saved_stem_audio_files': 0,
    }
    if args.six_decks:
        receipt.update({
            'six_live_decks_with_distinct_filters': 'passed',
            'cancel_one_five_continue': 'passed',
            'qualified_tempo_ratio': 1.16,
            'six_deck_playback_seconds': 90,
            'five_survivor_seconds': 8,
            'cuda_manifest_sha256': args.cuda_manifest_sha256,
            'model_processes_peak_limit': 1,
            'model_aggregate_enforced_quota_mib_limit': 6144,
        })
    print(json.dumps(receipt))


if __name__ == '__main__':
    main()
