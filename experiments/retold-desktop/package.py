#!/usr/bin/env python3
"""Build an unsigned macOS app bundle with a checksummed inventory, zip it, and verify it.

Usage:
  package.py build [--output DIR]   release build, bundle, inventory, archive and verification
  package.py verify ARCHIVE.zip     re-check archive digest, inventory and worker smoke probe
  package.py upgrade OLD.zip NEW.zip  initialize state with OLD, start NEW, and prove NEW refuses to
                                      reinitialize that state and leaves every existing file unchanged

Signing and notarization need release credentials and are deliberately not performed here.
"""
import argparse
import hashlib
import json
import platform
import plistlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "experiments/retold-desktop/Cargo.toml"
TARGET = ROOT / "target/gpui-g0"
APP_NAME = "Retold Studio.app"
EXECUTABLE = "retold-studio"
BUNDLE_ID = "dev.retold.studio"
INVENTORY = "inventory.json"


def version():
    return tomllib.loads(MANIFEST.read_text())["package"]["version"]


def commit():
    result = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True)
    return result.stdout.strip() or "unknown"


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def build_binary():
    subprocess.run(
        ["cargo", "build", "--manifest-path", str(MANIFEST), "--target-dir", str(TARGET),
         "--locked", "--release", "--bin", EXECUTABLE],
        cwd=ROOT, check=True,
    )
    return TARGET / "release" / EXECUTABLE


def info_plist(app_version):
    return {
        "CFBundleName": "Retold Studio",
        "CFBundleDisplayName": "Retold Studio",
        "CFBundleIdentifier": BUNDLE_ID,
        "CFBundleExecutable": EXECUTABLE,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": app_version,
        "CFBundleVersion": app_version,
        "LSMinimumSystemVersion": "13.0",
        "NSHighResolutionCapable": True,
    }


def write_inventory(app, metadata):
    files = sorted(path for path in app.rglob("*") if path.is_file() and path.name != INVENTORY)
    entries = [{"path": str(path.relative_to(app)), "bytes": path.stat().st_size,
                "sha256": sha256(path)} for path in files]
    inventory = dict(metadata, files=entries)
    (app / "Contents" / INVENTORY).write_text(json.dumps(inventory, indent=2) + "\n")
    return inventory


def bundle(binary, output, app_version):
    app = output / APP_NAME
    if app.exists():
        shutil.rmtree(app)
    (app / "Contents" / "MacOS").mkdir(parents=True)
    (app / "Contents" / "Resources").mkdir()
    shutil.copy2(binary, app / "Contents" / "MacOS" / EXECUTABLE)
    with open(app / "Contents" / "Info.plist", "wb") as handle:
        plistlib.dump(info_plist(app_version), handle)
    metadata = {"name": "Retold Studio", "version": app_version, "commit": commit(),
                "platform": f"macos-{platform.machine()}", "signed": False}
    write_inventory(app, metadata)
    return app


def archive(app, output, app_version):
    name = f"retold-studio-{app_version}-macos-{platform.machine()}.zip"
    path = output / name
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as handle:
        for file in sorted(app.rglob("*")):
            if file.is_file():
                info = zipfile.ZipInfo.from_file(file, arcname=str(file.relative_to(output)))
                with open(file, "rb") as source:
                    handle.writestr(info, source.read(), zipfile.ZIP_DEFLATED)
    (output / f"{name}.sha256").write_text(f"{sha256(path)}  {name}\n")
    return path


def check_inventory(app):
    inventory = json.loads((app / "Contents" / INVENTORY).read_text())
    listed = {entry["path"] for entry in inventory["files"]}
    present = {str(path.relative_to(app)) for path in app.rglob("*")
               if path.is_file() and path.name != INVENTORY}
    if listed != present:
        raise SystemExit(f"Inventory mismatch: missing {sorted(present - listed)}, extra {sorted(listed - present)}")
    for entry in inventory["files"]:
        if sha256(app / entry["path"]) != entry["sha256"]:
            raise SystemExit(f"Checksum mismatch: {entry['path']}")
    return inventory


def smoke(app):
    executable = app / "Contents" / "MacOS" / EXECUTABLE
    with tempfile.TemporaryDirectory() as scratch:
        probe = Path(scratch) / "probe"
        probe.mkdir()
        subprocess.run([str(executable), "--worker-probe", str(probe)], check=True, timeout=60)


def verify(path):
    checksum = path.with_name(path.name + ".sha256")
    if checksum.exists() and checksum.read_text().split()[0] != sha256(path):
        raise SystemExit("Archive digest does not match its .sha256 file")
    with tempfile.TemporaryDirectory() as scratch:
        with zipfile.ZipFile(path) as handle:
            handle.extractall(scratch)
        app = Path(scratch) / APP_NAME
        executable = app / "Contents" / "MacOS" / EXECUTABLE
        executable.chmod(0o755)
        inventory = check_inventory(app)
        smoke(app)
    print(f"Verified {path.name}: {len(inventory['files'])} files, version {inventory['version']}, "
          f"commit {inventory['commit'][:12]}, unsigned")


def extracted(archive, root):
    with zipfile.ZipFile(archive) as handle:
        handle.extractall(root)
    executable = Path(root) / APP_NAME / "Contents" / "MacOS" / EXECUTABLE
    executable.chmod(0o755)
    return executable


def snapshot(directory):
    return {str(path.relative_to(directory)): sha256(path)
            for path in sorted(directory.rglob("*")) if path.is_file()}


def upgrade(old, new):
    with tempfile.TemporaryDirectory() as scratch:
        scratch = Path(scratch)
        workspace = scratch / "workspace"
        workspace.mkdir()
        before_binary = extracted(old, scratch / "old")
        subprocess.run([str(before_binary), "--worker-probe", str(workspace)], check=True, timeout=60)
        before = snapshot(workspace)
        after_binary = extracted(new, scratch / "new")
        fresh = scratch / "fresh"
        fresh.mkdir()
        subprocess.run([str(after_binary), "--worker-probe", str(fresh)], check=True, timeout=60)
        refused = subprocess.run([str(after_binary), "--worker-probe", str(workspace)],
                                 capture_output=True, timeout=60)
        if refused.returncode == 0:
            raise SystemExit("The new build initialized over existing workspace state")
        after = snapshot(workspace)
        changed = [path for path, digest in before.items() if after.get(path) != digest]
        if changed:
            raise SystemExit(f"Upgrade changed existing workspace files: {changed}")
    print(f"Upgrade from {old.name} to {new.name} preserved {len(before)} workspace files")


def main(argv=None):
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    built = commands.add_parser("build")
    built.add_argument("--output", type=Path, default=ROOT / "target/gpui-g0/dist")
    checked = commands.add_parser("verify")
    checked.add_argument("archive", type=Path)
    upgraded = commands.add_parser("upgrade")
    upgraded.add_argument("old", type=Path)
    upgraded.add_argument("new", type=Path)
    arguments = parser.parse_args(argv)
    if arguments.command == "verify":
        verify(arguments.archive)
        return
    if arguments.command == "upgrade":
        upgrade(arguments.old, arguments.new)
        return
    if sys.platform != "darwin":
        raise SystemExit("The app bundle is macOS-only; other platforms are not yet qualified")
    arguments.output.mkdir(parents=True, exist_ok=True)
    app_version = version()
    app = bundle(build_binary(), arguments.output, app_version)
    verify(archive(app, arguments.output, app_version))


if __name__ == "__main__":
    main()
