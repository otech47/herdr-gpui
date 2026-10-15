"""Run: python3 -m unittest discover -s scripts/release/tests -v"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts/sign-local.sh"
ORIGINALS = ["/mock/login keychain-db", "/mock/system.keychain"]


class SignLocalTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="herdr-sign-local-test-")
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        tools = self.work / "tools"
        tools.mkdir()
        mock = tools / "mock.py"
        mock.write_bytes((ROOT / "scripts/release/tests/mock-tool.py").read_bytes())
        mock.chmod(0o755)
        for tool in ["uname", "security", "codesign", "openssl"]:
            (tools / tool).symlink_to(mock)
        self.log = self.work / "log"
        self.keychain = self.work / "keychains/herdr-local-signing.keychain-db"
        self.keychain.parent.mkdir()
        self.env = {
            "PATH": str(tools) + os.pathsep + os.environ["PATH"],
            "HOME": str(self.work),
            "TMPDIR": str(self.work),
            "MOCK_LOG": str(self.log),
            "HERDR_LOCAL_SIGNING_KEYCHAIN": str(self.keychain),
        }
        self.app = self.work / "Herdr.app"
        (self.app / "Contents/MacOS").mkdir(parents=True)
        (self.app / "Contents/Info.plist").write_text("<plist/>")
        (self.app / "Contents/MacOS/Herdr").write_bytes(b"never run")

    def run_script(self, *args, success=True):
        self.log.write_text("")
        result = subprocess.run(
            ["bash", str(SCRIPT), *map(str, args)],
            env=self.env, capture_output=True, text=True, timeout=30,
        )
        if success:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0)
        return result

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_first_run_creates_the_identity_and_later_runs_reuse_it(self):
        self.run_script(self.app)
        calls = self.calls()
        self.assertIn(["security", "create-keychain", "-p", "dummy-temporary-password", str(self.keychain)], calls)
        imports = [c for c in calls if c[:2] == ["security", "import"]]
        self.assertEqual(len(imports), 1)
        self.assertEqual(imports[0][-2:], ["-T", "/usr/bin/codesign"])
        partitions = [c for c in calls if c[:2] == ["security", "set-key-partition-list"]]
        self.assertEqual(len(partitions), 1)
        self.assertEqual(partitions[0][2:4], ["-S", "apple-tool:,apple:,codesign:"])
        self.assertEqual(partitions[0][-1], str(self.keychain))
        pkcs12 =[c for c in calls if c[:2] == ["openssl", "pkcs12"]]
        self.assertEqual(len(pkcs12), 1)
        for flag in ("-keypbe", "-certpbe", "-macalg"):
            self.assertIn(flag, pkcs12[0])
        self.assertFalse(list(self.work.glob("herdr-sign-local.*")))
        password = self.keychain.with_name("herdr-local-signing.password")
        self.assertEqual(password.stat().st_mode & 0o777, 0o600)
        self.assertEqual(password.read_text(), "dummy-temporary-password\n")
        searches =[c for c in calls if c[:5] == ["security", "list-keychains", "-d", "user", "-s"]]
        self.assertEqual(searches, [["security", "list-keychains", "-d", "user", "-s", *ORIGINALS, str(self.keychain)]])
        self.assertIn(["security", "unlock-keychain", "-p", "dummy-temporary-password", str(self.keychain)], calls)
        self.assertIn(["codesign", "--force", "--sign", "Herdr local signing", "--keychain", str(self.keychain), str(self.app)], calls)
        self.assertEqual(calls[-1], ["codesign", "--verify", "--strict", str(self.app)])
        self.assertTrue((self.app / "Contents/_CodeSignature/CodeResources").is_file())

        self.run_script(self.app)
        calls = self.calls()
        for subcommand in ("create-keychain", "import", "set-key-partition-list", "delete-keychain"):
            self.assertFalse([c for c in calls if c[:2] == ["security", subcommand]], subcommand)
        self.assertFalse([c for c in calls if c[0] == "openssl"])
        self.assertIn(["security", "unlock-keychain", "-p", "dummy-temporary-password", str(self.keychain)], calls)
        self.assertIn(["codesign", "--force", "--sign", "Herdr local signing", "--keychain", str(self.keychain), str(self.app)], calls)

    def test_a_keychain_without_its_password_is_made_again(self):
        self.run_script(self.app)
        self.keychain.with_name("herdr-local-signing.password").unlink()
        self.run_script(self.app)
        calls = self.calls()
        self.assertIn(["security", "delete-keychain", str(self.keychain)], calls)
        self.assertTrue([c for c in calls if c[:2] == ["security", "create-keychain"]])
        self.assertTrue([c for c in calls if c[:2] == ["security", "import"]])

    def test_failures_stop_before_signing(self):
        self.env["MOCK_FAIL"] = "codesign"
        self.run_script(self.app, success=False)
        self.assertFalse((self.app / "Contents/_CodeSignature").exists())
        del self.env["MOCK_FAIL"]
        self.env["MOCK_OS"] = "Linux"
        self.run_script(self.app, success=False)
        self.assertEqual(self.calls(), [])
        del self.env["MOCK_OS"]
        self.run_script(self.work, success=False)
        self.run_script(success=False)
        self.env["HERDR_LOCAL_SIGNING_NAME"] = 'Herdr "local"'
        self.run_script(self.app, success=False)
        self.assertEqual(self.calls(), [])


if __name__ == "__main__":
    unittest.main()
