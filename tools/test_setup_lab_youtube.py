"""No-network checks for the official-tool setup's checksum gate."""
import hashlib
import unittest
from unittest.mock import patch
import setup_lab_youtube as setup


class SetupTest(unittest.TestCase):
    def test_digest_verified_before_binary_is_returned(self):
        blob = b"test executable"
        info = {"assets": [{"name": "tool", "browser_download_url": "https://github.com/official/tool", "digest": "sha256:" + hashlib.sha256(blob).hexdigest()}]}
        with patch.object(setup, "fetch", return_value=blob):
            data, manifest = setup.verified_asset(info, "tool")
            self.assertEqual(data, blob)
            self.assertEqual(manifest["sha256"], hashlib.sha256(blob).hexdigest())
            info["assets"][0]["digest"] = "sha256:" + "0" * 64
            with self.assertRaises(RuntimeError):
                setup.verified_asset(info, "tool")

    def test_no_digest_or_checksum_never_installs(self):
        info = {"assets": [{"name": "tool", "browser_download_url": "https://github.com/official/tool"}]}
        with patch.object(setup, "fetch", return_value=b"unverified"):
            with self.assertRaises(RuntimeError):
                setup.verified_asset(info, "tool")


if __name__ == "__main__":
    unittest.main()
