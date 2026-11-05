#!/usr/bin/env python3
"""Unknown nested properties/rcon commands only; no handler or I/O request."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
p=argparse.ArgumentParser();p.add_argument('--zig',required=True);p.add_argument('--rust',required=True);a=p.parse_args()
if os.name!='posix': raise SystemExit('POSIX raw argv corpus only')
binaries=[str(Path(x).resolve()) for x in [a.zig,a.rust]]
count=0
for top, group, known in [(b'server',b'properties',b'get'),(b'server',b'rcon',b'command'),
    (b'instance',b'mods',b'list'),(b'instance',b'resourcepacks',b'list'),(b'instance',b'shaderpacks',b'list'),
    (b'instance',b'datapacks',b'list'),(b'instance',b'modpack',b'install'),(b'instance',b'worlds',b'list'),
    (b'server',b'worlds',b'list'),(b'server',b'mods',b'list'),(b'server',b'datapacks',b'list')]:
    for raw in [b'\xff',b'\xed\xa0\x80']:
        for tail in [[raw],[b'unknown',raw],[b'',raw],[b'--help',raw],[b'unknown',known,raw]]:
            outputs=[]
            for binary in binaries:
                with tempfile.TemporaryDirectory(prefix='owned-nested-usage-') as td:
                    r=subprocess.run([binary,top,group]+tail,cwd=td,capture_output=True,timeout=10)
                    assert not list(Path(td).iterdir())
                    outputs.append((r.returncode,r.stdout,r.stderr))
            assert outputs[0]==outputs[1],(group,tail,outputs)
            assert outputs[0][0]==(1 if top==b'server' and group in [b'mods',b'datapacks'] and os.uname().sysname!='Linux' else 2) and not outputs[0][1]
            count+=1
print(f'{count} unknown nested group cases match exact status/stdout/stderr; no writes')
