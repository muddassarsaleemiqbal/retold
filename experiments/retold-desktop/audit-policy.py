#!/usr/bin/env python3.11
"""Keep the approved desktop exceptions pinned and every other root policy rule intact."""
import argparse
import copy
from pathlib import Path
import tomllib

DESKTOP = Path(__file__).resolve().parent
ROOT = DESKTOP.parent.parent
APPROVED = {
    "RUSTSEC-2024-0384": ("instant", "0.1.13"),
    "RUSTSEC-2024-0436": ("paste", "1.0.15"),
    "RUSTSEC-2025-0134": ("rustls-pemfile", "2.2.0"),
    "RUSTSEC-2026-0206": ("rustybuzz", "0.20.1"),
    "RUSTSEC-2026-0192": ("ttf-parser", "0.25.1"),
    "RUSTSEC-2026-0371": ("zbus_names", "4.3.5"),
    "RUSTSEC-2026-0370": ("zvariant", "5.15.1"),
    "RUSTSEC-2026-0372": ("zvariant_derive", "5.15.1"),
}
LICENSES = [("enum-iterator", "2.3.0"), ("enum-iterator-derive", "1.5.0")]


def read(path):
    return tomllib.loads(path.read_text())


def validate(policy_path, lock_path):
    base = read(ROOT / "deny.toml")
    policy = read(policy_path)
    normalized = copy.deepcopy(policy)
    ignored = normalized["advisories"].pop("ignore")
    normalized["advisories"]["ignore"] = base["advisories"]["ignore"]
    ids = [entry["id"] for entry in ignored]
    if set(ids) != set(APPROVED) or len(ids) != len(APPROVED):
        raise ValueError("Desktop advisory exceptions differ from the approved eight IDs")
    if not all(entry.get("reason", "").strip() for entry in ignored):
        raise ValueError("Every desktop advisory exception needs its review reason")
    licenses = normalized["licenses"].pop("exceptions")
    expected = [{"name": name, "version": "=" + version, "allow": ["0BSD"]} for name, version in LICENSES]
    if licenses != expected:
        raise ValueError("0BSD exceptions must remain restricted to the two exact package versions")
    if normalized != base:
        raise ValueError("Desktop policy differs from the root beyond approved exceptions; reconcile the policy")
    packages = read(lock_path)["package"]
    pins = [*APPROVED.values(), *LICENSES, ("gpui-kit", "0.8.0")]
    for name, version in pins:
        found = {package["version"] for package in packages if package["name"] == name}
        if found != {version}:
            raise ValueError(f"Review desktop exception again: {name} expected {version}, found {sorted(found)}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--policy", type=Path, default=DESKTOP / "deny.toml")
    parser.add_argument("--lock", type=Path, default=DESKTOP / "Cargo.lock")
    arguments = parser.parse_args()
    try:
        validate(arguments.policy, arguments.lock)
    except (ValueError, KeyError, TypeError, OSError) as error:
        raise SystemExit(f"Desktop dependency policy rejected: {error}") from error
    print("Desktop exception IDs, exact package versions and root-policy parity passed")
