import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from android_rc import identity, manifest, native_matches, NATIVE_WORKFLOW


class IdentityTests(unittest.TestCase):
    PROPERTIES = "versionName=1.1.0-rc.1\nversionCode=43\n"

    def test_artifact_and_publish_tags_share_one_embedded_identity(self):
        proof = identity(self.PROPERTIES, "refs/tags/android-rc-build-v1.1.0-rc.1")
        self.assertTrue(proof["artifact_only"])
        self.assertEqual(proof["version_code"], 43)
        self.assertFalse(identity(self.PROPERTIES, "refs/tags/v1.1.0-rc.1")["artifact_only"])

    def test_stable_mismatched_and_unreviewed_refs_are_rejected(self):
        for ref in ["refs/tags/v1.1.0", "refs/tags/v1.1.0-rc.2", "refs/heads/feature/test"]:
            with self.assertRaises(ValueError):
                identity(self.PROPERTIES, ref)
        with self.assertRaises(ValueError):
            identity("versionName=1.1.0\nversionCode=44", "refs/tags/v1.1.0")
        with self.assertRaises(ValueError):
            identity(self.PROPERTIES, "refs/heads/release/test", "v1.1.0-rc.2")


class ActualGitNativeGateTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        (self.root / "android").mkdir()
        (self.root / "android/song.java").write_text("base")
        workflow = self.root / NATIVE_WORKFLOW
        workflow.parent.mkdir(parents=True)
        workflow.write_text("verified native workflow")
        self.commit("base")
        self.git("checkout", "-b", "feature")
        (self.root / "android/song.java").write_text("candidate")
        self.commit("head")
        self.head = self.git("rev-parse", "HEAD")
        self.git("checkout", "main")
        (self.root / "README").write_text("base documentation")
        self.commit("diverged base")
        self.git("merge", "--no-ff", "feature", "-m", "tested PR merge")
        self.tested = self.git("rev-parse", "HEAD")
        self.run = {"status": "completed", "conclusion": "success", "path": NATIVE_WORKFLOW,
                    "repository": {"full_name": "gitwide-cloud/amp-verse"},
                    "event": "pull_request", "head_sha": self.head}

    def tearDown(self):
        self.directory.cleanup()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True, stderr=subprocess.DEVNULL).strip()

    def commit(self, name):
        self.git("add", ".")
        self.git("commit", "-m", name)

    def test_actual_tested_merge_is_qualified_and_docs_do_not_invalidate_runtime(self):
        self.assertTrue(native_matches(self.run, self.root, self.tested))
        (self.root / "README").write_text("release notes")
        self.commit("notes")
        self.assertTrue(native_matches(self.run, self.root, self.tested))

    def test_runtime_drift_and_forged_tested_source_cannot_reuse_a_green_run(self):
        self.assertFalse(native_matches(self.run, self.root, self.head))
        (self.root / "android/song.java").write_text("untested audio")
        self.commit("drift")
        self.assertFalse(native_matches(self.run, self.root, self.tested))

    def test_failed_or_wrong_workflow_run_is_not_accepted(self):
        for change in [{"conclusion": "failure"}, {"status": "in_progress"}, {"path": "unrelated.yml"},
                       {"repository": {"full_name": "other/repository"}}]:
            self.assertFalse(native_matches({**self.run, **change}, self.root, self.tested))

    def test_test_verifier_drift_in_pr_head_is_rejected(self):
        self.git("checkout", "feature")
        (self.root / NATIVE_WORKFLOW).write_text("skip native test")
        self.commit("unverified workflow")
        altered = {**self.run, "head_sha": self.git("rev-parse", "HEAD")}
        self.git("checkout", "main")
        self.assertFalse(native_matches(altered, self.root, self.tested))


class ManifestTests(unittest.TestCase):
    def test_only_matching_signed_package_identity_is_described(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            apk = root / "amp-full.apk"
            apk.write_bytes(b"fixture signed APK bytes; signature verified by upstream packager")
            entry = {"name": apk.name, "sha256": hashlib.sha256(apk.read_bytes()).hexdigest(),
                     "version": "1.1.0-rc.1", "version_code": 43, "signer_sha256": "a" * 64}
            (root / "android-packages.json").write_text(json.dumps({"version": entry["version"], "packages": {"android-full-arm64": entry}}))
            proof = root / "gate.json"
            proof.write_text(json.dumps({"version": entry["version"], "version_code": 43, "rc": 1,
                                        "source_commit": "b" * 40, "native_run_id": 1}))
            result = manifest(root, proof)
            self.assertEqual(result["release_channel"], "rc")
            self.assertEqual(result["packages"]["android-full-arm64"]["version_code"], 43)
            apk.write_bytes(b"corrupted")
            with self.assertRaises(ValueError):
                manifest(root, proof)


if __name__ == "__main__":
    unittest.main()
