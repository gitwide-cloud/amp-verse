import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from package_android_modules import release_identity
from package_downloads import desktop_version, write_desktop_manifest
from update_manifest import generate


class UpdateManifestTests(unittest.TestCase):
    def test_complete_versioned_release_and_hashes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for platform in ('windows', 'linux'):
                (root / f'amp-verse-{platform}-v1.2.3.zip').write_bytes(b'package')
            apk = root / 'amp-verse-1.2.3-b42-release.apk'
            apk.write_bytes(b'apk')
            result = generate(root, '1.2.3', apk, 42)
            self.assertEqual(len(result['packages']), 3)
            self.assertEqual(result['packages']['android-arm64']['version_code'], 42)
            self.assertEqual(result['packages']['linux-x86_64']['sha256'], hashlib.sha256(b'package').hexdigest())

    def test_release_candidate_uses_product_identity_and_updater_asset(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "android").mkdir()
            (root / "Cargo.toml").write_text('[workspace.package]\nversion = "1.1.0-rc.1"\n')
            properties = root / "android/release.properties"
            properties.write_text("versionName=1.1.0-rc.1\nversionCode=43\n")
            with patch.dict(os.environ, {"GITHUB_REF": "refs/heads/release", "GITHUB_SHA": "1234567890"}):
                output = root / "build.json"
                write_desktop_manifest(root, output)
                self.assertEqual(json.loads(output.read_text()), {
                    "source_sha": "1234567890",
                    "version": "1.1.0-rc.1",
                    "release_channel": "rc",
                    "package_version": "1.1.0-rc.1+git.12345678",
                })
                for platform in ("windows", "linux"):
                    (root / f"amp-verse-{platform}-v1.1.0-rc.1.zip").write_bytes(b"package")
                    (root / f"amp-server-{platform}-v1.1.0-rc.1.zip").write_bytes(b"server")
                manifest = generate(root, "1.1.0-rc.1")
                self.assertEqual(manifest["version"], "1.1.0-rc.1")
                self.assertIn("server-windows-x86_64", manifest["packages"])
                properties.write_text("versionName=1.0.22\nversionCode=42\n")
                with self.assertRaisesRegex(ValueError, "product versions disagree"):
                    desktop_version(root)

    def test_android_package_accepts_rc_and_preserves_other_channels(self):
        self.assertEqual(release_identity("1.1.0-rc.1"), {"release_channel": "rc", "rc": 1})
        self.assertEqual(release_identity("0.3.2-beta.6"), {"release_channel": "beta", "beta": 6})
        self.assertEqual(release_identity("1.1.0"), {"release_channel": "stable"})
        with self.assertRaises(ValueError):
            release_identity("1.1.0-rc.0")

    def test_stable_tag_names_match_assets_selected_by_updater(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "android").mkdir()
            (root / "Cargo.toml").write_text('[workspace.package]\nversion = "1.1.0"\n')
            (root / "android/release.properties").write_text("versionName=1.1.0\nversionCode=44\n")
            with patch.dict(os.environ, {"GITHUB_REF": "refs/tags/v1.1.0", "GITHUB_SHA": "1234567890"}):
                version = desktop_version(root)
                for platform in ("windows", "linux"):
                    (root / f"amp-verse-{platform}-v{version}.zip").write_bytes(b"package")
                result = generate(root, version)
                self.assertEqual(result["packages"]["windows-x86_64"]["name"], "amp-verse-windows-v1.1.0.zip")
                self.assertEqual(result["packages"]["linux-x86_64"]["name"], "amp-verse-linux-v1.1.0.zip")
                output = root / "build.json"
                write_desktop_manifest(root, output)
                self.assertEqual(json.loads(output.read_text())["release_channel"], "stable")
            with patch.dict(os.environ, {"GITHUB_REF": "refs/tags/v1.1.1"}):
                with self.assertRaisesRegex(ValueError, "tag does not match"):
                    desktop_version(root)

    def test_every_android_edition_and_engine_is_addressable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for platform in ("windows", "linux"):
                (root / f"amp-verse-{platform}-v1.1.0-rc.11.zip").write_bytes(b"desktop")
                (root / f"amp-server-{platform}-v1.1.0-rc.11.zip").write_bytes(b"server")
            (root / "amp-engine-windows-x86_64.exe").write_bytes(b"engine")
            editions = {}
            for edition in ("full", "lite", "dj", "lab", "cast", "podcasts", "remote"):
                path = root / f"amp-{edition}.apk"
                path.write_bytes(edition.encode())
                editions[f"android-{edition}-arm64"] = {"name":path.name,"sha256":hashlib.sha256(path.read_bytes()).hexdigest(),"size":path.stat().st_size,"version_code":55}
            editions["android-arm64"] = editions["android-full-arm64"]
            (root / "android-packages.json").write_text(json.dumps({"packages":editions}))
            result = generate(root, "1.1.0-rc.11")
            self.assertEqual(len(result["packages"]), 13)
            self.assertIn("android-dj-arm64", result["packages"])
            self.assertIn("engine-windows-x86_64", result["packages"])
            self.assertIn("server-linux-x86_64", result["packages"])

    def test_missing_platform_does_not_publish_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                generate(Path(directory), '1.2.3')
            self.assertFalse((Path(directory) / 'amp-update.json').exists())

    def test_unstable_version_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                generate(Path(directory), '../1.2.3')


if __name__ == '__main__':
    unittest.main()
