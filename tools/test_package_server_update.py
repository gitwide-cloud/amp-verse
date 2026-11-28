import json, tempfile, unittest, zipfile
from pathlib import Path
from package_server_update import package

class ServerUpdateTests(unittest.TestCase):
    def test_package_contains_core_only_and_preserves_executable_bits(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);source=root/"source";source.mkdir()
            for name in ["amp-cast","amp-server-update","amp-prepare","LICENSE","THIRD_PARTY_NOTICES.md"]:(source/name).write_bytes(b"binary")
            (source/"build.json").write_text(json.dumps({"source_sha":"a"*40}))
            (source/"server.json").write_bytes(b"private-settings")
            (source/"manager").mkdir();(source/"manager/private.txt").write_bytes(b"private")
            archive=root/"server.zip";names=package(source,archive)
            with zipfile.ZipFile(archive) as zipped:
                self.assertEqual(set(names),set(zipped.namelist()))
                self.assertNotIn("server.json",names)
                self.assertEqual((zipped.getinfo("amp-server-update").external_attr>>16)&0o777,0o755)
    def test_missing_helper_rejects_package(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/"build.json").write_text(json.dumps({"source_sha":"a"*40}));(root/"amp-cast.exe").write_bytes(b"app")
            with self.assertRaisesRegex(ValueError,"Incomplete server update"):
                package(root,root/"update.zip")
            self.assertFalse((root/"update.zip").exists())
