#!/usr/bin/env python3
"""POSIX-only raw argv rejection corpus. Every case must fail before planning."""
import argparse
import os
import pathlib
import subprocess
import tempfile

p = argparse.ArgumentParser()
p.add_argument('--zig', required=True)
p.add_argument('--rust', required=True)
a = p.parse_args()
if os.name != 'posix':
    raise SystemExit('This raw POSIX argv corpus does not claim Windows coverage')
binaries = [os.fsencode(pathlib.Path(s).resolve()) for s in [a.zig, a.rust]]
tails = [[b'--directory'], [b'--directory', b''], [b'--arch', b'bad'],
         [b'--source', b'bad'], [b'--os', b'bad'], [b'--workers', b'1'],
         [b'--fabric-loader'], [b'--forge-version', b''],
         [b'--neoforge-version', b'v', b'--arch', b'bad']]
for tail in tails:
    results = []
    for binary in binaries:
        with tempfile.TemporaryDirectory(prefix='jmcl-prepare-cli-') as root:
            r = subprocess.run([binary, b'install', b'prepare', b'\xff'] + tail,
                               cwd=root, capture_output=True, timeout=10, check=False)
            assert not list(pathlib.Path(root).iterdir()), ('unexpected write', tail)
            results.append((r.returncode, r.stdout, r.stderr))
    assert results[0] == results[1], (tail, results)
    assert results[0][0] == 2 and not results[0][1], (tail, results)
print(f'{len(tails)} raw argv cases passed: exact status/stdout/stderr; no disk writes')
