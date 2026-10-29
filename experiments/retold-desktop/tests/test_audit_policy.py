"""Regression checks that policy changes cannot silently broaden approved exceptions."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

DESKTOP = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("desktop_audit_policy", DESKTOP / "audit-policy.py")
POLICY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(POLICY)


class DesktopPolicyTests(unittest.TestCase):
    def validate(self, policy_text=None, lock_text=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            policy = root / "deny.toml"
            lock = root / "Cargo.lock"
            policy.write_text(policy_text if policy_text is not None else (DESKTOP / "deny.toml").read_text())
            lock.write_text(lock_text if lock_text is not None else (DESKTOP / "Cargo.lock").read_text())
            POLICY.validate(policy, lock)

    def test_approved_policy_passes(self):
        self.validate()

    def test_global_license_allowance_is_rejected(self):
        text = (DESKTOP / "deny.toml").read_text().replace('allow = ["MIT",', 'allow = ["0BSD", "MIT",', 1)
        with self.assertRaises(ValueError):
            self.validate(policy_text=text)

    def test_another_advisory_exception_is_rejected(self):
        text = (DESKTOP / "deny.toml").read_text().replace('ignore = [', 'ignore = [{ id = "RUSTSEC-2026-0386", reason = "not approved" },', 1)
        with self.assertRaises(ValueError):
            self.validate(policy_text=text)

    def test_filtering_platform_maintenance_checks_is_rejected(self):
        text = (DESKTOP / "deny.toml").read_text().replace('unmaintained = "all"', 'unmaintained = "workspace"', 1)
        with self.assertRaises(ValueError):
            self.validate(policy_text=text)

    def test_exception_dependency_upgrade_is_rejected(self):
        text = (DESKTOP / "Cargo.lock").read_text().replace('name = "instant"\nversion = "0.1.13"', 'name = "instant"\nversion = "0.1.14"', 1)
        with self.assertRaises(ValueError):
            self.validate(lock_text=text)

    def test_versionless_license_exception_is_rejected(self):
        text = (DESKTOP / "deny.toml").read_text().replace('version = "=2.3.0"\n', '', 1)
        with self.assertRaises(ValueError):
            self.validate(policy_text=text)


if __name__ == "__main__":
    unittest.main()
