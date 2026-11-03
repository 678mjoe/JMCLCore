#!/usr/bin/env python3
"""Owned POSIX list/show raw argv corpus; no create/delete/install invocation."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
p = argparse.ArgumentParser()
p.add_argument('--zig', required=True)
p.add_argument('--rust', required=True)
a = p.parse_args()
if os.name != 'posix':
    raise SystemExit('POSIX argv fixture only; no Windows execution claim')
binaries = [os.fsencode(Path(v).resolve()) for v in [a.zig, a.rust]]
def snapshot(root):
    return {str(p.relative_to(root)): (p.lstat().st_mode, p.lstat().st_ino,
            p.lstat().st_mtime_ns, p.lstat().st_nlink,
            os.readlink(p) if p.is_symlink() else p.read_bytes() if p.is_file() else None)
            for p in [root, *root.rglob('*')]}
with tempfile.TemporaryDirectory(prefix='owned-instance-read-cli-') as td:
    root = Path(td)
    native = b'instances-\xff'
    store = root / os.fsdecode(native)
    (store / 'demo').mkdir(parents=True)
    (store / 'demo' / 'instance.json').write_bytes(b'{"id":"demo","name":[255,65],"version_id":"fake"}')
    (store / 'aaa').mkdir()
    (store / 'aaa' / 'instance.json').write_bytes(b'{"id":"aaa","name":"First","version_id":"fake"}')
    (store / 'broken').mkdir()
    (store / 'broken' / 'instance.json').write_bytes(b'{bad')
    link = b'link-\xff'
    (root / os.fsdecode(link)).symlink_to(os.fsdecode(native), target_is_directory=True)
    (root / '--json').mkdir()
    blocked = b'blocked-\xff' 
    (root / os.fsdecode(blocked)).write_bytes(b'canary')
    (root / 'canary').write_bytes(b'untouched')
    cases = []
    for command, ids in [(b'list', []), (b'show', [b'demo'])]:
        for path in [native, b'./' + native + b'//', native + b'/./']:
            for mode in [[], [b'--json']]:
                cases.append(([command] + ids + [b'--instances-directory', path] + mode, 0))
        cases.append(([command] + ids + [b'--instances-directory', blocked], 1))
        cases.append(([command] + ids + [b'--instances-directory', link], 1))
        for tail in [[b'--unknown'], [b'--instances-directory'], [b'--instances-directory', b''], [b'--force'], [b'\xff']]:
            cases.append(([command] + ids + [b'--instances-directory', native] + tail, 2))
        cases.append(([command] + ids + [b'--instances-directory', blocked, b'--instances-directory', native, b'--json', b'--json'], 0))
    cases += [([b'show', x, b'--instances-directory', native], 2) for x in [b'', b'\xff', b'../escape', b'CON', b'bad.']]
    cases += [([b'list', b'--instances-directory', b'absent-\xff', b'--force'], 2),
              ([b'show', b'\xff', b'--instances-directory', b'absent-\xff'], 2)]
    cases += [([b'show', b'missing', b'--instances-directory', native], 1)]
    cases.append(([b'list', b'--instances-directory', native, b'--instances-directory', b'--json', b'--json'], 0))
    before = snapshot(root)
    for argv, status in cases:
        results = []
        for binary in binaries:
            r = subprocess.run([binary, b'instance'] + argv, cwd=root, capture_output=True, timeout=10)
            assert snapshot(root) == before, ('unexpected disk mutation', argv)
            results.append((r.returncode, r.stdout, r.stderr))
        assert results[0] == results[1], (argv, results)
        assert results[0][0] == status, (argv, results)
    print(f'{len(cases)} cases passed: exact status/stdout/stderr and unchanged owned trees')

# Valid reads create a missing root, as in the original. Exercise only owned,
# isolated locations and require exactly the expected empty directory effect.
for command, ids, expected in [(b'list', [], 0), (b'show', [b'missing'], 1)]:
    results = []
    for binary in binaries:
        with tempfile.TemporaryDirectory(prefix='owned-instance-root-creation-') as td:
            root = Path(td)
            r = subprocess.run([binary, b'instance', command] + ids + [b'--instances-directory', b'new-\xff'], cwd=root, capture_output=True, timeout=10)
            results.append((r.returncode, r.stdout, r.stderr))
            made = root / os.fsdecode(b'new-\xff')
            assert list(root.iterdir()) == [made] and made.is_dir() and not list(made.iterdir())
    assert results[0] == results[1] and results[0][0] == expected, results
print('2 missing-root cases passed: exact output and only expected root creation')
