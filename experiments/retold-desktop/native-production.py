#!/usr/bin/env python3
"""Run zero-price production fixtures through native generation, approval and export."""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "experiments/retold-desktop/Cargo.toml"
TARGET = ROOT / "target/gpui-g0"


def main():
    if os.name != "posix":
        raise SystemExit("Native production fixture requires the Unix backend")
    build = subprocess.run(
        ["cargo", "test", "--manifest-path", str(MANIFEST), "--target-dir", str(TARGET),
         "--locked", "--test", "shell_native", "--no-run", "--message-format=json"],
        cwd=ROOT, capture_output=True, text=True, check=False,
    )
    sys.stderr.write(build.stderr)
    build.check_returncode()
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.strip()]
    drivers = [artifact["executable"] for artifact in artifacts
               if artifact.get("reason") == "compiler-artifact"
               and artifact.get("target", {}).get("name") == "shell_native"
               and artifact.get("executable")]
    if len(drivers) != 1:
        raise SystemExit("Expected exactly one compiled native interaction driver")
    environment = os.environ.copy()
    worker = TARGET / "debug/retold-studio"
    if not worker.is_file():
        raise SystemExit("Build the packaged retold-studio worker before acceptance")
    environment.update(RETOLD_NATIVE_DRIVER=drivers[0],
                       RETOLD_NATIVE_WORKER=str(worker), SDL_AUDIODRIVER="dummy")
    subprocess.run(["cargo", "clippy", "--locked", "-p", "retold-cli", "--test",
                    "production_journey", "--features", "native-acceptance", "--", "-D", "warnings"],
                   cwd=ROOT, env=environment, check=True)
    command = ["cargo", "test", "--locked", "-p", "retold-cli", "--test",
               "production_journey", "--features", "native-acceptance",
               "native_desktop_", "--"]
    listing = subprocess.run(command + ["--list"], cwd=ROOT, env=environment,
                             capture_output=True, text=True, check=False)
    sys.stderr.write(listing.stderr)
    listing.check_returncode()
    expected = {"native_desktop_exports_ready_video: test",
                "native_desktop_runs_through_character_review_and_exports: test"}
    if not expected.issubset(listing.stdout.splitlines()):
        raise SystemExit("Native production acceptance tests were not discovered")
    subprocess.run(command + ["--test-threads=1", "--nocapture"],
                   cwd=ROOT, env=environment, check=True)


if __name__ == "__main__":
    main()
