import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from package_windows_exe import verified_package


class PackageVerification(unittest.TestCase):
    def test_tampering_and_traversal_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'payload').mkdir()
            (root / 'support').mkdir()
            (root / 'install-windows-package.ps1').write_bytes(b'installer')
            song = root / 'payload' / 'amp-player.exe'
            song.write_bytes(b'verified build')
            manifest = dict(schema=1, role='Desktop', bootstrap_sha256=hashlib.sha256(b'installer').hexdigest(), files=[dict(path='amp-player.exe', sha256=hashlib.sha256(song.read_bytes()).hexdigest())], support_files=[])
            def save():
                (root / 'install-manifest.json').write_text(json.dumps(manifest))
            save()
            verified_package(root)
            song.write_bytes(b'changed build')
            with self.assertRaises(ValueError):
                verified_package(root)
            manifest['files'][0]['path'] = '../outside.exe'
            save()
            with self.assertRaises(ValueError):
                verified_package(root)


if __name__ == '__main__':
    unittest.main()
