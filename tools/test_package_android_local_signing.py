import unittest
from package_android_local_signing import identity
class IdentityTest(unittest.TestCase):
 def test_rc_release_identity(self):
  result=identity("package: name='cloud.gitwide.ampverse' versionCode='43' versionName='1.1.0-rc.1'","cloud.gitwide.ampverse","1.1.0-rc.1",43)
  self.assertFalse(result["debuggable"])
 def test_reject_debuggable(self):
  with self.assertRaises(ValueError):identity("package: name='cloud.gitwide.ampverse' versionCode='43' versionName='1.1.0-rc.1'\napplication-debuggable","cloud.gitwide.ampverse","1.1.0-rc.1",43)
 def test_reject_wrong_code_and_module(self):
  for badging in ("package: name='cloud.gitwide.ampverse' versionCode='42' versionName='1.1.0-rc.1'","package: name='cloud.gitwide.ampverse.cast' versionCode='43' versionName='1.1.0-rc.1'"):
   with self.assertRaises(ValueError):identity(badging,"cloud.gitwide.ampverse","1.1.0-rc.1",43)
if __name__=="__main__":unittest.main()
