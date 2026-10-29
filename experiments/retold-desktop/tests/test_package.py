"""Package inventories detect missing, extra and modified files."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("package", Path(__file__).resolve().parents[1] / "package.py")
package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package)


def app_with(root):
    app = Path(root) / package.APP_NAME
    (app / "Contents" / "MacOS").mkdir(parents=True)
    (app / "Contents" / "MacOS" / package.EXECUTABLE).write_bytes(b"binary")
    package.write_inventory(app, {"version": "0", "commit": "c", "signed": False})
    return app


class InventoryTest(unittest.TestCase):
    def test_an_untouched_bundle_verifies(self):
        with tempfile.TemporaryDirectory() as root:
            inventory = package.check_inventory(app_with(root))
            self.assertEqual(len(inventory["files"]), 1)
            self.assertFalse(inventory["signed"])

    def test_modified_extra_and_missing_files_are_rejected(self):
        for change in ("modify", "extra", "missing"):
            with tempfile.TemporaryDirectory() as root:
                app = app_with(root)
                binary = app / "Contents" / "MacOS" / package.EXECUTABLE
                if change == "modify":
                    binary.write_bytes(b"tampered")
                elif change == "extra":
                    (app / "Contents" / "extra").write_bytes(b"x")
                else:
                    binary.unlink()
                with self.assertRaises(SystemExit):
                    package.check_inventory(app)


if __name__ == "__main__":
    unittest.main()
