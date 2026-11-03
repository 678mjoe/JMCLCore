#!/usr/bin/env python3
"""Invalid-syntax CLI cases only, which must stop before any server root access."""
import argparse
from pathlib import Path
import subprocess
import sys
import tempfile
p = argparse.ArgumentParser()
p.add_argument('--zig', required=True)
p.add_argument('--rust', required=True)
a = p.parse_args()
if sys.platform != "linux":
    raise SystemExit("Linux-only server CLI rejection corpus")
binaries = [str(Path(v).resolve()) for v in [a.zig, a.rust]]
cases = [["demo", "--bogus=value"], ["demo", "--json=true"],
         ["demo", "--servers-directory=x"], ["demo"] + ["a=b"] * 1025]
for args in cases:
    outputs = []
    for binary in binaries:
        with tempfile.TemporaryDirectory(prefix='owned-property-usage-') as td:
            r = subprocess.run([binary, 'server', 'properties', 'set'] + args,
                               cwd=td, capture_output=True, timeout=10)
            assert not list(Path(td).iterdir()), 'unexpected root/write before argument rejection'
            outputs.append((r.returncode, r.stdout, r.stderr))
    assert outputs[0] == outputs[1], outputs
    assert outputs[0][0] == 2 and not outputs[0][1], outputs
print('4 invalid syntax cases match exact status/stdout/stderr; no root or property writes')
