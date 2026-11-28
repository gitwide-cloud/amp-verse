import json
from pathlib import Path
import tempfile
import unittest
from engine_release_metadata import write_metadata


class MetadataTests(unittest.TestCase):
    def test_contract_for_both_platforms(self):
        root = Path(__file__).resolve().parents[1]
        for platform, asset in [("windows-x86_64", "amp-engine-windows-x86_64.exe"),
                                ("linux-x86_64", "amp-engine-linux-x86_64")]:
            with tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                with self.assertRaises(ValueError):
                    write_metadata(root, directory, platform, "v0.3.2")
                (directory / asset).write_bytes(b"fixture")
                (directory / (asset + ".sha256")).write_text("fixture checksum")
                write_metadata(root, directory, platform, "v0.3.2")
                metadata = json.loads((directory / (asset + ".json")).read_text())
                self.assertEqual(metadata["asset"], asset)
                self.assertEqual(metadata["platform"], platform)
                self.assertEqual(metadata["protocol"], 1)
                self.assertEqual(metadata["engine_id"], "amp-prep-heuristic-5")
                self.assertEqual(metadata["release"], "v0.3.2")
                write_metadata(root, directory, platform, "v1.1.0-rc.11")
                self.assertEqual(json.loads((directory / (asset+".json")).read_text())["release"],"v1.1.0-rc.11")
                for tag in ["v0.3.2-rc.0", "v01.2.3", "../../bad", "0.3.2"]:
                    with self.assertRaises(ValueError):
                        write_metadata(root, directory, platform, tag)


if __name__ == "__main__":
    unittest.main()
