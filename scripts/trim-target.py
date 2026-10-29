#!/usr/bin/env python3
"""Delete build artifacts that no current build uses, keeping `target/` small.

Every rebuild after a dependency or flag change leaves the old test binaries, libraries and object
files behind under a new hash; on this workspace they reach tens of gigabytes. Cargo reports the
artifacts of the current builds (all already fresh, so this is quick); anything in `deps/` or
`examples/` carrying another hash is stale and removed.
"""
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
PROFILE = ROOT / 'target' / 'debug'
HASH = re.compile(r'-([0-9a-f]{16})(?=[.]|$)')
BUILDS = [
    ['cargo', 'build', '--workspace', '--all-targets'],
    ['cargo', 'test', '--workspace', '--no-run'],
    ['cargo', 'clippy', '--workspace', '--all-targets'],
]


def current_hashes() -> set[str]:
    hashes = set()
    for build in BUILDS:
        output = subprocess.run(
            [*build, '--message-format=json', '--quiet'],
            cwd=ROOT, capture_output=True, text=True, check=True,
        ).stdout
        for line in output.splitlines():
            message = json.loads(line)
            if message.get('reason') != 'compiler-artifact':
                continue
            for name in [*message.get('filenames', []), message.get('executable') or '']:
                found = HASH.search(pathlib.Path(name).name)
                if found:
                    hashes.add(found.group(1))
    return hashes


def main() -> int:
    if not PROFILE.is_dir():
        return 0
    keep = current_hashes()
    freed = 0
    for folder in (PROFILE / 'deps', PROFILE / 'examples'):
        for path in folder.glob('*'):
            found = HASH.search(path.name)
            if not found or found.group(1) in keep:
                continue
            if path.is_dir():
                freed += sum(f.stat().st_size for f in path.rglob('*') if f.is_file())
                subprocess.run(['rm', '-rf', str(path)], check=True)
            else:
                freed += path.stat().st_size
                path.unlink()
    print(f'trimmed {freed / 1e9:.1f} GB of stale build artifacts')
    return 0


if __name__ == '__main__':
    sys.exit(main())
