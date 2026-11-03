#!/usr/bin/env python3
"""Only pure top-level usage/help/unknown-command cases, never operational commands."""
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
    raise SystemExit('POSIX argv only; not native Windows evidence')
binaries = [os.fsencode(Path(v).resolve()) for v in [a.zig, a.rust]]
cases = []
for raw in [b'\xff', b'bad-\xed\xa0\x80', b'prefix\n\xff\tend']:
    for command in [b'help', b'--help', b'rpc', b'version', b'unknown']:
        cases.append([command, raw])
    cases.extend([[raw], [raw, b'--help'], [b'version', b'--json', raw],
                  [b'version', raw, b'--json'], [b'version', b'unknown', raw]])
for argv in cases:
    results = []
    for binary in binaries:
        with tempfile.TemporaryDirectory(prefix='owned-cli-diagnostics-') as td:
            root = Path(td)
            canary = root / 'canary'
            canary.write_bytes(b'untouched')
            before = canary.stat()
            r = subprocess.run([binary] + argv, cwd=root, capture_output=True,
                               input=b'', timeout=10)
            assert list(root.iterdir()) == [canary]
            after = canary.stat()
            assert canary.read_bytes() == b'untouched' and before.st_mtime_ns == after.st_mtime_ns and before.st_ino == after.st_ino
            results.append((r.returncode, r.stdout, r.stderr))
    assert results[0] == results[1], (argv, results)
    assert results[0][0] == (0 if argv[0] in [b'help', b'--help'] else 2)
print(f'{len(cases)} exact status/stdout/stderr cases passed; no filesystem writes')
