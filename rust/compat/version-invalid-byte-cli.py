#!/usr/bin/env python3
"""Finite malformed version and plan arguments only; never a valid metadata request."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
p=argparse.ArgumentParser();p.add_argument('--zig',required=True);p.add_argument('--rust',required=True);a=p.parse_args()
if os.name != 'posix': raise SystemExit('POSIX byte argv corpus only')
binaries=[str(Path(x).resolve()) for x in [a.zig,a.rust]]
cases=[]
for raw in [b'\xff',b'\xed\xa0\x80']:
    cases += [[b'version',b'list',raw], [b'version',b'list',b'--json',raw]]
    for opt in [b'--source',b'--type',b'--limit']:
        cases += [[b'version',b'list',opt,raw], [b'version',b'list',opt,raw,opt,b'1']]
    cases += [[b'version',b'show',b'',raw],[b'version',b'show',b'1.21',raw],
              [b'version',b'show',b'1.21',b'--source',raw],
              [b'version',b'show',raw,b'--source'],
              [b'version',b'show',raw,b'--unknown'],
              [b'version',b'show',raw,b'--source',b'bad',b'--source',b'official']]
    cases += [[b'install',b'plan',raw,b'--source'],
              [b'install',b'plan',raw,b'--os',b'bad',b'--os',b'linux'],
              [b'install',b'plan',b'',raw],
              [b'install',b'plan',b'x',raw]]
    for opt in [b'--source',b'--os',b'--arch']:
        cases += [[b'install',b'plan',b'x',opt,raw]]
    for opt in [b'--fabric-loader',b'--neoforge-version',b'--forge-version']:
        cases += [[b'install',b'plan',b'x',opt,raw,b'--unknown'],
                  [b'install',b'plan',raw,opt,b'',opt,b'v']]
for argv in cases:
    outputs=[]
    for binary in binaries:
        with tempfile.TemporaryDirectory(prefix='owned-version-invalid-') as td:
            r=subprocess.run([binary]+argv,cwd=td,capture_output=True,timeout=10)
            assert not list(Path(td).iterdir()),'unexpected filesystem write'
            outputs.append((r.returncode,r.stdout,r.stderr))
    assert outputs[0]==outputs[1],(argv,outputs)
    assert outputs[0][0]==2 and not outputs[0][1],outputs
print(f'{len(cases)} invalid native version/plan cases match exact status/stdout/stderr; no writes')
