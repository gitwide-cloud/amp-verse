"""Offline CLI/result-contract tests only: mocked output is NOT native qualification."""
import array
import contextlib
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import subprocess
import unittest
from unittest.mock import patch
import verify_live_stem_player as verify


class QualificationCliTests(unittest.TestCase):
    def parse(self, *args):
        return verify.parse_args(["--module", "unused-fixture-module", *args])

    def rejects(self, *args):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            self.parse(*args)
        self.assertEqual(error.exception.code, 2)

    def test_default_ci_profile_stays_short_with_original_input_length(self):
        args = self.parse()
        self.assertFalse(args.sustained_cpu)
        self.assertFalse(args.six_decks)
        self.assertEqual(args.source_seconds, 66)

    def test_sustained_cpu_is_explicit_and_has_enough_input(self):
        args = self.parse("--sustained-cpu")
        self.assertTrue(args.sustained_cpu)
        self.assertEqual(args.source_seconds, 220)

    def test_short_source_bounds_accept_edges_reject_outside(self):
        for seconds in [54, 600]:
            self.assertEqual(self.parse("--source-seconds", str(seconds)).source_seconds, seconds)
        for invalid in ["53", "601", "0", "-1", "999999", "nan"]:
            self.rejects("--source-seconds", invalid)

    def test_sustained_source_minimum_is_enforced(self):
        self.assertEqual(self.parse("--sustained-cpu", "--source-seconds", "210").source_seconds, 210)
        self.rejects("--sustained-cpu", "--source-seconds", "209")

    def test_gpu_qualification_never_silently_skips_missing_configuration(self):
        self.rejects("--six-decks")
        self.rejects("--sustained-cpu", "--six-decks")
        self.rejects("--cuda-module", "unused-cuda")

    def test_combined_explicit_gpu_and_sustained_cpu_remain_enabled(self):
        args = self.parse("--sustained-cpu", "--six-decks",
                          "--cuda-module", "unused-cuda", "--cuda-manifest-sha256", "a" * 64)
        self.assertTrue(args.sustained_cpu)
        self.assertTrue(args.six_decks)
        self.assertEqual(args.source_seconds, 220)

    def test_four_deck_gpu_profile_is_explicit_and_excludes_six(self):
        pins = ["--cuda-module", "unused-cuda", "--cuda-manifest-sha256", "a" * 64]
        self.rejects("--four-decks")
        self.rejects("--four-decks", "--six-decks", *pins)
        self.assertEqual(self.parse("--four-decks", *pins).gpu_decks, 4)
        self.assertEqual(self.parse("--six-decks", *pins).gpu_decks, 6)

    def test_gpu_source_minimum_is_enforced(self):
        options = ["--six-decks", "--cuda-module", "unused-cuda",
                   "--cuda-manifest-sha256", "a" * 64]
        self.assertEqual(self.parse(*options, "--source-seconds", "180").source_seconds, 180)
        self.rejects(*options, "--source-seconds", "179")

    def fake_result(self, text, expected=None, code=0):
        with patch.object(verify.subprocess, "run", return_value=subprocess.CompletedProcess(
                ["FAKE-test-executable"], code, stdout=text)), contextlib.redirect_stdout(io.StringIO()):
            verify.checked_test(["FAKE-test-executable"], {}, expected_report=expected)

    def test_zero_matched_tests_cannot_be_qualified(self):
        with self.assertRaisesRegex(RuntimeError, "zero matched"):
            self.fake_result("test result: ok. 0 passed; 1 filtered out")

    def test_old_short_executable_cannot_be_relabelled_sustained(self):
        with self.assertRaisesRegex(RuntimeError, "requested CPU duration"):
            self.fake_result("Actual concurrent live decks:30sec together+6sec B-only\n"
                             "test result: ok. 1 passed;",
                             "Actual concurrent live decks:180sec together+15sec B-only")

    def test_success_requires_passed_test_and_correct_duration(self):
        report = "Actual concurrent live decks:180sec together+15sec B-only"
        self.fake_result(report + "\ntest result: ok. 1 passed;", report)
        with self.assertRaises(subprocess.CalledProcessError):
            self.fake_result(report + "\ntest result: FAILED. 0 passed;", report, code=101)


class InternalExportTests(unittest.TestCase):
    def test_export_is_opt_in_and_requires_cargo_provenance_and_logs(self):
        args = verify.parse_args(["--module", "unused"])
        self.assertFalse(args.export_test_binary)
        for options in [
            ["--export-test-binary"],
            ["--export-test-binary", "--log-dir", "unused", "--test-binary", "old.exe"],
        ]:
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                verify.parse_args(["--module", "unused", *options])

    def fixture(self, root, windows=False):
        source = root / "apps/amp-player/src/main.rs"
        source.parent.mkdir(parents=True)
        source.write_text("synthetic source")
        executable = root / ("test.exe" if windows else "test-binary")
        executable.write_bytes(b"synthetic executable, never run")
        record = {
            "reason": "compiler-artifact", "package_id": "fixture-amp-player",
            "profile": {"test": True, "opt_level": "3"},
            "target": {"name": "amp-player", "kind": ["bin"], "src_path": str(source)},
            "executable": str(executable),
        }
        return record, executable

    def test_tracked_dirty_source_is_rejected_and_clean_pin_ignores_generated_outputs(self):
        with patch.object(verify.subprocess, "check_output", side_effect=["tracked", " M tracked.rs"]):
            with self.assertRaisesRegex(RuntimeError, "clean tracked"):
                verify.tracked_checkout(Path("unused"))
        with patch.object(verify.subprocess, "check_output", side_effect=["tracked", "", "a" * 40, "b" * 40]) as git:
            self.assertEqual(verify.tracked_checkout(Path("unused")),
                             {"commit": "a" * 40, "tree": "b" * 40})
            self.assertIn("--untracked-files=no", git.call_args_list[1].args[0])

    def test_authoritative_current_test_artifact_is_selected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            record, executable = self.fixture(root)
            self.assertEqual(verify.cargo_test_artifact([{"reason": "build-finished"}, record], root),
                             (record, executable))

    def test_zero_ambiguous_wrong_target_or_profile_artifacts_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            record, _ = self.fixture(root)
            wrong = [
                [],
                [record, record],
                [dict(record, profile={"test": False})],
                [dict(record, target=dict(record["target"], name="amp-update"))],
                [dict(record, target=dict(record["target"], kind=["lib"]))],
                [dict(record, target=dict(record["target"], src_path=str(root / "other.rs")))],
            ]
            for records in wrong:
                with self.subTest(records=records), self.assertRaises(RuntimeError):
                    verify.cargo_test_artifact(records, root)

    def test_relative_missing_or_linked_executable_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            record, executable = self.fixture(root)
            for path in ["relative-test", str(root / "missing-test")]:
                with self.assertRaises(RuntimeError):
                    verify.cargo_test_artifact([dict(record, executable=path)], root)
            link = root / "linked-test"
            try:
                link.symlink_to(executable)
            except OSError:
                return  # Windows runners may lack symlink permission; other path cases still run.
            with self.assertRaises(RuntimeError):
                verify.cargo_test_artifact([dict(record, executable=str(link))], root)

    def test_failed_packaged_cpu_acceptance_cannot_export(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            module = root / "module"
            module.mkdir()
            paths = {"worker": "worker", "model": "model", "runtime": "runtime"}
            for name in [*paths.values(), "module.json"]:
                (module / name).write_bytes(b"synthetic fixture; validation mocked")
            logs = root / "logs"
            pin = {"commit": "a" * 40, "tree": "b" * 40}
            # Eight synthetic seconds suffice for the 23-second preparation copy.
            silence = array.array("f", [0.]) * (44100 * 2 * 8)
            with patch.object(verify, "tracked_checkout", return_value=pin), \
                    patch.object(verify, "checkout_identity", return_value=pin), \
                    patch.object(verify, "validate_module", return_value=(module, {"relative_paths": paths})), \
                    patch.object(verify, "fixture", return_value=silence), \
                    patch.object(verify, "checked_test", side_effect=[None, None, RuntimeError("synthetic CPU failure")]), \
                    patch.object(verify, "export_test_binary") as export, self.assertRaisesRegex(RuntimeError, "CPU failure"):
                verify.main(["--module", str(module), "--log-dir", str(logs), "--export-test-binary"])
            export.assert_not_called()
            receipt = json.loads((logs / "qualification.json").read_text())
            self.assertEqual(receipt["status"], "failed")
            self.assertEqual(len(receipt["tests_passed"]), 2)
            self.assertEqual(receipt["cargo_profile"], "release")

    def test_no_export_does_not_invoke_cargo_or_git(self):
        with patch.object(verify.subprocess, "run") as cargo, patch.object(verify, "tracked_checkout") as git:
            self.assertIsNone(verify.export_test_binary(
                SimpleNamespace(export_test_binary=False), None, None, {}, Path("unused")))
            cargo.assert_not_called()
            git.assert_not_called()

    def test_windows_export_pins_matching_dll_and_source_without_running_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            record, executable = self.fixture(root, windows=True)
            out = root / "out"
            out.mkdir()
            (out / "amp-keylock.dll").write_bytes(b"synthetic keylock")
            logs = root / "logs"
            logs.mkdir()
            records = [record, {"reason": "build-script-executed",
                                "package_id": record["package_id"], "out_dir": str(out)}]
            pin = {"commit": "a" * 40, "tree": "b" * 40}
            result = subprocess.CompletedProcess([], 0,
                stdout="\n".join(json.dumps(r) for r in records), stderr="")
            with patch.object(verify, "tracked_checkout", return_value=pin), \
                    patch.object(verify.subprocess, "run", return_value=result) as cargo:
                export = verify.export_test_binary(
                    SimpleNamespace(export_test_binary=True), logs, pin, {}, root)
            self.assertIn("--release", cargo.call_args.args[0])
            self.assertIn("--no-run", cargo.call_args.args[0])
            self.assertEqual(export["source"], pin)
            destination = logs / "internal-test-binary"
            self.assertEqual((destination / "amp-player-acceptance.exe").read_bytes(), executable.read_bytes())
            self.assertEqual((destination / "amp-keylock.dll").read_bytes(), b"synthetic keylock")
            self.assertEqual(len(export["files"]), 2)
            self.assertEqual(json.loads((destination / "provenance.json").read_text()), export)

    def test_missing_keylock_or_stale_sibling_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            record, executable = self.fixture(root, windows=True)
            out = root / "out"
            out.mkdir()
            logs = root / "logs"
            logs.mkdir()
            pin = {"commit": "a" * 40, "tree": "b" * 40}
            records = [record, {"reason": "build-script-executed",
                                "package_id": record["package_id"], "out_dir": str(out)}]
            result = subprocess.CompletedProcess([], 0,
                stdout="\n".join(json.dumps(r) for r in records), stderr="")
            for stale in [False, True]:
                if stale:
                    (out / "amp-keylock.dll").write_bytes(b"current")
                    (executable.parent / "amp-keylock.dll").write_bytes(b"stale")
                with patch.object(verify, "tracked_checkout", return_value=pin), \
                        patch.object(verify.subprocess, "run", return_value=result), \
                        self.assertRaises(RuntimeError):
                    verify.export_test_binary(SimpleNamespace(export_test_binary=True), logs, pin, {}, root)
                self.assertFalse((logs / "internal-test-binary").exists())

    def test_failed_or_malformed_cargo_metadata_never_publishes_executable(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            logs = root / "logs"
            logs.mkdir()
            pin = {"commit": "a" * 40}
            for code, output, error in [
                (1, "", subprocess.CalledProcessError),
                (0, "not-json", json.JSONDecodeError),
                (0, "", RuntimeError),
            ]:
                result = subprocess.CompletedProcess([], code, stdout=output, stderr="bounded fixture")
                with patch.object(verify, "tracked_checkout", return_value=pin), \
                        patch.object(verify.subprocess, "run", return_value=result), self.assertRaises(error):
                    verify.export_test_binary(SimpleNamespace(export_test_binary=True), logs, pin, {}, root)
                self.assertFalse((logs / "internal-test-binary").exists())

    def test_changed_tracked_source_prevents_cargo_export(self):
        with patch.object(verify, "tracked_checkout", return_value={"commit": "new"}), \
                patch.object(verify.subprocess, "run") as cargo, self.assertRaises(RuntimeError):
            verify.export_test_binary(SimpleNamespace(export_test_binary=True), None,
                                      {"commit": "old"}, {}, Path("unused"))
        cargo.assert_not_called()


if __name__ == "__main__":
    unittest.main()
