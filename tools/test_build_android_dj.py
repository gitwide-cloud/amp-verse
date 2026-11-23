"""Offline cross-build boundary checks; subprocesses are mocked, not native-code proof."""
import os
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("build_android_dj.py")


class NativeAbiPolicy(unittest.TestCase):
    def run_builder(self, enabled):
        calls, copies = [], []
        original = Path.is_file
        with patch.dict(os.environ, {"AMP_UI_TEST_X86": enabled,
                                     "ANDROID_NDK_HOME": "/tmp/amp-ui-test-ndk"}), \
                patch.object(Path, "is_file", lambda p: p.name in ("clang", "clang.exe") or original(p)), \
                patch.object(Path, "mkdir"), \
                patch("subprocess.run", lambda args, **kw: calls.append((args, kw["env"]))), \
                patch("shutil.copy2", lambda source, dest: copies.append((source, dest))):
            runpy.run_path(str(SCRIPT), run_name="__main__")
        return calls, copies

    def test_production_default_builds_only_arm64(self):
        calls, copies = self.run_builder("0")
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][0][calls[0][0].index("--target") + 1], "aarch64-linux-android")
        self.assertIn("--locked", calls[0][0])
        self.assertIn("arm64-v8a", str(copies[0][1]))
        self.assertNotIn("x86_64", str(copies[0][1]))

    def test_explicit_ui_optin_adds_separate_emulator_library(self):
        calls, copies = self.run_builder("1")
        self.assertEqual(len(calls), 2)
        for (args, env), (source, dest), (target, abi, clang) in zip(calls, copies, (
                ("aarch64-linux-android", "arm64-v8a", "aarch64-linux-android28"),
                ("x86_64-linux-android", "x86_64", "x86_64-linux-android28"))):
            self.assertEqual(args[args.index("--target") + 1], target)
            self.assertIn(target, str(source))
            self.assertIn(abi, str(dest))
            self.assertIn("--target=" + clang, env["CARGO_ENCODED_RUSTFLAGS"])
            self.assertIn("CARGO_TARGET_" + target.upper().replace("-", "_") + "_LINKER", env)


if __name__ == "__main__":
    unittest.main()
