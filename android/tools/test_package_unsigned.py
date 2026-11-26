import unittest
from package_unsigned import valid_version

class VersionTest(unittest.TestCase):
    def test_published_channel_versions(self):
        for value in ("0.3.2-beta.6", "1.1.0-rc.1", "1.1.0-rc.2", "1.1.0"):
            with self.subTest(value=value): self.assertTrue(valid_version(value))
    def test_rejects_unversioned_or_malformed_builds(self):
        for value in ("1.1.0-preview.1", "1.1.0-rc.0", "1.1.0-rc.02", "01.1.0", "1.1.0-rc.2/../../x"):
            with self.subTest(value=value): self.assertFalse(valid_version(value))

if __name__ == "__main__": unittest.main()
