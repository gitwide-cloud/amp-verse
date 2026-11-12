import hashlib
from pathlib import Path
import tempfile
import unittest
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
