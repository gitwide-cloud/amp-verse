import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import package_windows_installers as subject


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.app = self.root / 'application'
        self.app.mkdir()
        (self.app / 'build.json').write_text(json.dumps({'source_sha': 'a' * 40, 'files': []}))
        (self.app / 'amp-cast.exe').write_bytes(b'synthetic not executable')
        self.crt = self.root / 'crt'
        (self.crt / 'notices').mkdir(parents=True)
        redist = self.crt / 'notices/vc_redist.x64.exe'
        redist.write_bytes(b'original license container fixture')
        files = []
        for name in ('msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'):
            (self.crt / name).write_bytes(name.encode())
            files.append({'name': name, 'sha256': subject.digest(self.crt / name),
                          'signer': 'CN=Microsoft; O=Microsoft Corporation'})
        subject.write_json(self.crt / 'CRT-PROVENANCE.json', {
            'files': files, 'original_redist_sha256': subject.digest(redist)})
        self.archive = self.root / 'codec.zip'
        with zipfile.ZipFile(self.archive, 'w') as archive:
            for name in ('LICENSE', 'README.txt'):
                archive.writestr('ffmpeg-9.1-essentials_build/' + name, name)
        self.output = self.root / 'output'

    def package(self):
        with patch.object(subject, 'verify_bytes', return_value=b'checked'), patch.object(
                subject, 'selected_binaries', return_value=[(n, b'fixture') for n in
                ('ffmpeg.exe', 'ffprobe.exe', 'ffplay.exe')]):
            return subject.package('Server', self.app, None, self.archive, self.crt, self.output, '123')

    def test_inventory_and_defaults(self):
        receipt = self.package()
        self.assertEqual(receipt['source_commit'], 'a' * 40)
        self.assertEqual(receipt['run_id'], '123')
        for entry in receipt['files']:
            self.assertEqual(subject.digest(self.output / 'payload' / entry['path']), entry['sha256'])
        launcher = (self.output / 'payload/Start AMP Verse Server.ps1').read_text()
        self.assertIn('-Mode Background', launcher)
        self.assertNotIn('ProtectedMode', (self.output / 'Install AMP Verse.cmd').read_text())

    def test_changed_crt_rejected(self):
        (self.crt / 'vcruntime140.dll').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'CRT payload differs'):
            self.package()
        self.assertFalse(self.output.exists())

    def test_license_container_required(self):
        (self.crt / 'notices/vc_redist.x64.exe').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'license container changed'):
            self.package()

    def test_missing_crt_rejected(self):
        receipt = json.loads((self.crt / 'CRT-PROVENANCE.json').read_text())
        receipt['files'].pop()
        subject.write_json(self.crt / 'CRT-PROVENANCE.json', receipt)
        with self.assertRaisesRegex(ValueError, 'Complete app-local CRT'):
            self.package()

    def test_never_overwrites_evidence(self):
        self.output.mkdir()
        with self.assertRaisesRegex(ValueError, 'must not exist'):
            self.package()

    def test_source_identity_required(self):
        (self.app / 'build.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'source identity'):
            self.package()


if __name__ == '__main__':
    unittest.main()
