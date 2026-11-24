"""Offline CLI/result-contract tests only: mocked output is NOT native qualification."""
import contextlib
import io
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


if __name__ == "__main__":
    unittest.main()
