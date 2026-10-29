#!/usr/bin/env python3
"""Enforce one-way dependencies and reachability from the production CLI.

Crates the binary cannot reach are dead weight that tests can keep green forever. Any such crate
must be listed in EXPERIMENTAL with the reason it is not wired yet; the list is checked both ways
so it cannot go stale once a crate is wired in.
"""
import json
import subprocess
import re
from pathlib import Path

base = {'retold-core'}
allowed = {
    'retold-core': set(),
    'retold-infer': base | {'retold-providers'},
    'retold-render': base | {'retold-providers'},
    'retold-algo': base,
    'retold-guide': base,
    'retold-lang': base,
    'retold-providers': base,
    'retold-filmcraft': base,
    'retold-dramaturgy': base | {'retold-algo', 'retold-lang'},
    'retold-parallax': base | {'retold-infer', 'retold-render'},
    'retold-deliver': base,
    'retold-lipsync': base | {'retold-algo', 'retold-lang'},
    'retold-voice': base | {'retold-infer', 'retold-providers', 'retold-lang'},
    'retold-engine': base | {
        'retold-algo', 'retold-guide', 'retold-lang', 'retold-providers',
        'retold-filmcraft', 'retold-dramaturgy', 'retold-parallax', 'retold-infer', 'retold-render', 'retold-voice',
        'retold-lipsync', 'retold-deliver',
    },
}
allowed['retold-app'] = {'retold-engine'}
allowed['retold-cli'] = {'retold-app'}
# Crates not yet reachable from the `retold-cli` package. Remove an entry when you wire the crate in.
EXPERIMENTAL = {}
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--format-version', '1', '--no-deps']))
violations = []
for package in metadata['packages']:
    if package['name'] == 'retold-app':
        for dependency in package['dependencies']:
            if dependency['name'] in {'clap', 'ratatui', 'crossterm'}:
                violations.append(f"retold-app must not import presentation dependency {dependency['name']}")
    if package['name'] not in allowed:
        violations.append(f"{package['name']} has no layer rule in scripts/check-architecture.py")
        continue
    for dependency in package['dependencies']:
        if dependency['kind'] is None and dependency['name'] in allowed and dependency['name'] != package['name'] and dependency['name'] not in allowed[package['name']]:
            violations.append(f"{package['name']} must not import {dependency['name']}")
workspace = {package['name']: package for package in metadata['packages']}
reachable, frontier = set(), ['retold-cli']
while frontier:
    name = frontier.pop()
    if name in reachable:
        continue
    reachable.add(name)
    frontier.extend(
        d['name'] for d in workspace[name]['dependencies']
        if d['name'] in workspace and d.get('kind') is None
    )
for name in sorted(workspace):
    if name not in reachable and name not in EXPERIMENTAL:
        violations.append(f"{name} is not reachable from the retold binary; wire it in or list it in EXPERIMENTAL")
    if name in reachable and name in EXPERIMENTAL:
        violations.append(f"{name} is now wired into retold; remove it from EXPERIMENTAL")
# Reducers and renderers must keep filesystem, processes and network access on workers.
tui = Path('crates/retold-cli/src/tui')
pure_surfaces = [tui / 'app.rs', *sorted((tui / 'app').glob('*.rs')), *sorted((tui / 'view').glob('*.rs'))]
blocking_io = re.compile(r"std::(?:fs|net)|std::process::(?:Command|Child|Stdio)|(?:reqwest|ureq)::|\.(?:canonicalize|metadata|symlink_metadata|read_dir|is_file|is_dir|exists)\s*\(")
for source in pure_surfaces:
    for number, line in enumerate(source.read_text().splitlines(), 1):
        code = line.split('//', 1)[0]
        if blocking_io.search(code):
            violations.append(f"{source}:{number}: blocking I/O belongs on a TUI worker")
if violations:
    raise SystemExit('\n'.join(violations))
print('Workspace dependency layers and binary reachability passed')
