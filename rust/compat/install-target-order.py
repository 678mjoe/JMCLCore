#!/usr/bin/env python3
"""Only invalid targets: rejection before planning, transport or installation."""
import argparse
from pathlib import Path
import subprocess
import tempfile
p = argparse.ArgumentParser()
p.add_argument('--zig', required=True)
p.add_argument('--rust', required=True)
a = p.parse_args()
binaries = [str(Path(v).resolve()) for v in [a.zig, a.rust]]
cases = [('--os', 'bad', 'linux'), ('--os', '', 'linux'),
         ('--arch', 'bad', 'amd64'), ('--arch', '', 'aarch64'),
         ('--os', 'linux', 'bad'), ('--arch', 'amd64', 'bad')]
for method in ['execute', 'prepare']:
    for flag, first, second in cases:
        outputs = []
        for binary in binaries:
            with tempfile.TemporaryDirectory(prefix='owned-target-rejection-') as td:
                r = subprocess.run([binary, 'install', method, 'fake', flag, first, flag, second],
                                   cwd=td, capture_output=True, timeout=10)
                assert not list(Path(td).iterdir()), 'unexpected preflight write'
                outputs.append((r.returncode, r.stdout, r.stderr))
        assert outputs[0] == outputs[1], (method, flag, first, second, outputs)
        assert outputs[0][0] == 2 and not outputs[0][1], outputs
print('12 invalid target-order cases match exact status/stdout/stderr; no writes')
