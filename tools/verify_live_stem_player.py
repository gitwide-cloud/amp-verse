"""Verify real live playback and opt-in compressed playlist preparation; temporary test audio is cleaned up."""
import argparse
import array
import hashlib
import math
import json
import os
from pathlib import Path
import subprocess
import tempfile
import wave

from test_live_stems import fixture, validate_module


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--module', type=Path, required=True)
    parser.add_argument('--six-decks', action='store_true',
                        help='Run hardware qualification: six real-model decks at 116%% for 90 seconds, then five survivors for 8 seconds. Requires a qualified CUDA GPU; CPU runs do not qualify.')
    parser.add_argument('--cuda-module', type=Path,
                        help='Pinned CUDA runtime bundle, required only with --six-decks.')
    parser.add_argument('--cuda-manifest-sha256',
                        help='Expected SHA-256 of the CUDA module manifest; required with --six-decks.')
    parser.add_argument('--sustained-cpu', action='store_true',
                        help='Opt in to CPU2 playback for 180 seconds plus 15 seconds with one survivor; defaults stay 30+6 seconds.')
    parser.add_argument('--source-seconds', type=int,
                        help='Synthetic input duration, bounded to 600 seconds; minimum depends on the selected qualification.')
    parser.add_argument('--test-binary', type=Path,
                        help='Optional full-release amp-player test executable built from this test source; its hash is recorded, not its source provenance inferred.')
    parser.add_argument('--log-dir', type=Path,
                        help='New or empty directory for per-test logs and qualification.json (including failed runs).')
    args = parser.parse_args(argv)
    if args.six_decks and (args.cuda_module is None or not args.cuda_manifest_sha256):
        parser.error('--six-decks requires --cuda-module and --cuda-manifest-sha256')
    if not args.six_decks and (args.cuda_module is not None or args.cuda_manifest_sha256):
        parser.error('CUDA bundle options are used only with --six-decks')
    minimum_source = max(210 if args.sustained_cpu else 54, 180 if args.six_decks else 0)
    if args.source_seconds is None:
        args.source_seconds = 220 if args.sustained_cpu or args.six_decks else 66
    if not minimum_source <= args.source_seconds <= 600:
        parser.error(f'--source-seconds must be between {minimum_source} and 600 for this qualification')
    return args


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def checkout_identity():
    root = Path(__file__).resolve().parents[1]
    try:
        commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True,
                                         stderr=subprocess.DEVNULL).strip()
        dirty = bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True,
                                            stderr=subprocess.DEVNULL).strip())
        return {'commit': commit, 'dirty': dirty}
    except (OSError, subprocess.CalledProcessError):
        return {'commit': None, 'dirty': None}


def checked_test(command, env, log_path=None, expected_report=None):
    result = subprocess.run(command, env=env, cwd=Path(__file__).resolve().parents[1],
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, check=False)
    if log_path:
        log_path.write_text(result.stdout, encoding='utf-8')
    print(result.stdout, end='')
    if result.returncode:
        raise subprocess.CalledProcessError(result.returncode, command)
    if 'test result: ok. 1 passed;' not in result.stdout:
        raise RuntimeError('Exact qualification must execute and pass one test; zero matched tests is not acceptance')
    if expected_report and expected_report not in result.stdout:
        raise RuntimeError('Test executable did not report the requested CPU duration; do not relabel a short run as sustained')


def main(argv=None):
    args = parse_args(argv)
    if args.test_binary is not None:
        args.test_binary = args.test_binary.resolve()
        if not args.test_binary.is_file():
            raise ValueError('--test-binary must name an existing compiled test executable')
    logs = args.log_dir.resolve() if args.log_dir else None
    if logs:
        logs.mkdir(parents=True, exist_ok=True)
        if any(logs.iterdir()):
            raise ValueError('--log-dir must be new or empty; preserve earlier evidence')
    module = args.module.resolve()
    module, manifest = validate_module(module)
    env = os.environ.copy()
    env['AMP_ONNX_WORKER'] = str(module / manifest['relative_paths']['worker'])
    env['AMP_ONNX_MODEL'] = str(module / manifest['relative_paths']['model'])
    env['ORT_DYLIB_PATH'] = str(module / manifest['relative_paths']['runtime'])
    env['AMP_ONNX_RUNTIME'] = env['ORT_DYLIB_PATH']
    env['AMP_ONNX_PROVIDER'] = 'cpu'
    env['AMP_STEM_TEST_CPU_PROFILE'] = 'sustained' if args.sustained_cpu else 'short'
    if args.sustained_cpu:
        env['AMP_STEM_TRACE'] = '1'
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
            chunk = integers.tobytes()
            remaining = args.source_seconds * 44100
            for _ in range(math.ceil(remaining / (len(chunk) // 4))):
                frames = min(remaining, len(chunk) // 4)
                output.writeframesraw(chunk[:frames * 4])
                remaining -= frames
        preparation_source = Path(directory) / 'original-selected-playlist-23sec.wav'
        with wave.open(str(preparation_source), 'wb') as output:
            output.setparams((2, 2, 44100, 0, 'NONE', 'not compressed'))
            output.writeframes((integers.tobytes() * 3)[:23 * 44100 * 4])
        with wave.open(str(preparation_source), 'rb') as check:
            assert check.getnframes() == 23 * 44100
        source_before = sha256_file(source)
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
        identity = {
            'status': 'running',
            'test_checkout': checkout_identity(),
            'acceptance_source_sha256': sha256_file(Path(__file__).resolve().parents[1] / 'apps/amp-player/src/mixer/live_stem_acceptance.rs'),
            'cpu_profile': env['AMP_STEM_TEST_CPU_PROFILE'],
            'cpu_together_seconds': 180 if args.sustained_cpu else 30,
            'cpu_survivor_seconds': 15 if args.sustained_cpu else 6,
            'synthetic_input_seconds': args.source_seconds,
            'synthetic_input_sha256': source_before,
            'module_manifest_sha256': sha256_file(module / 'module.json'),
            'packaged_components': {key: sha256_file(module / relative)
                                    for key, relative in manifest['relative_paths'].items()},
            'test_binary_sha256': sha256_file(args.test_binary) if args.test_binary else None,
            'test_binary_source_provenance': 'Operator must verify separately; a binary hash does not establish source identity.',
            'cargo_profile': None if args.test_binary else ('release' if args.sustained_cpu else 'default'),
            'tests_passed': [],
            'limits': 'Synthetic callback/model proof, not physical output or real-music listening acceptance.',
        }
        def save_identity():
            if logs:
                (logs / 'qualification.json').write_text(json.dumps(identity, indent=2), encoding='utf-8')
        save_identity()
        try:
            for index, (test, ignored, test_env) in enumerate(tests, 1):
                run_env = test_env.copy()
                run_env['AMP_STEM_TEST_SOURCE'] = str(source)
                if args.test_binary:
                    command = [str(args.test_binary), test]
                else:
                    command = ['cargo', 'test', '-p', 'amp-player', '--bin', 'amp-player', test, '--locked']
                    if args.sustained_cpu:
                        command.append('--release')
                    command.append('--')
                if ignored:
                    command.append('--ignored')
                command.extend(['--exact', '--test-threads=1', '--nocapture'])
                expected_report = None
                if test.endswith('two_native_live_stem_decks_play_filters_and_cancel_independently'):
                    expected_report = f'Actual concurrent live decks:{identity["cpu_together_seconds"]}sec together+{identity["cpu_survivor_seconds"]}sec B-only'
                checked_test(command, run_env, logs / f'{index:02d}-{test.split("::")[-1]}.log' if logs else None,
                             expected_report=expected_report)
                identity['tests_passed'].append(test)
                save_identity()
            assert sha256_file(source) == source_before, 'Original synthetic input was changed'
            assert sorted(p.name for p in Path(directory).iterdir()) == sorted([source.name, preparation_source.name])
        except Exception as error:
            identity.update(status='failed', failure=str(error))
            save_identity()
            raise
    assert not Path(directory).exists(), 'Temporary qualification input was not cleaned up'
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
        'original_synthetic_input_unchanged': True,
        'temporary_fixture_cleanup': True,
        'underruns': 0,
        'dry_fallback': False,
    }
    receipt.update(identity)
    receipt['status'] = 'passed'
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
    if logs:
        (logs / 'qualification.json').write_text(json.dumps(receipt, indent=2), encoding='utf-8')
    print(json.dumps(receipt))


if __name__ == '__main__':
    main()
