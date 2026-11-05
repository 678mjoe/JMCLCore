#!/usr/bin/env python3
"""Only absent/unknown instance/install/launch/store/server subcommands; no operational request."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
p = argparse.ArgumentParser()
p.add_argument('--zig', required=True)
p.add_argument('--rust', required=True)
a = p.parse_args()
binaries = [str(Path(v).resolve()) for v in [a.zig, a.rust]]
for command in ['instance', 'install', 'launch', 'store', 'server']:
    for tail in [[], [''], ['unknown'], ['--help']]:
        outputs = []
        for binary in binaries:
            with tempfile.TemporaryDirectory(prefix='owned-group-usage-') as td:
                r = subprocess.run([binary, command] + tail, cwd=td, capture_output=True, timeout=10)
                assert not list(Path(td).iterdir()), 'unexpected write during usage'
                outputs.append((r.returncode, r.stdout, r.stderr))
        assert outputs[0] == outputs[1], (command, tail, outputs)
        assert outputs[0][0] == 2 and not outputs[0][1], outputs
print('20 group usage cases match exact status/stdout/stderr; no writes')

if os.name == 'posix':
    for command in ['instance', 'install', 'launch', 'store', 'server']:
        for tail in [[b'\xff'], [b'\xed\xa0\x80'], [b'unknown', b'\xff'], [b'--help', b'\xff'], [b'', b'\xff'], [b'unknown', {'store': b'gc', 'server': b'start'}.get(command, b'execute'), b'\xff']]:
            outputs=[]
            for binary in binaries:
                with tempfile.TemporaryDirectory(prefix='owned-raw-group-usage-') as td:
                    r=subprocess.run([binary,command]+tail,cwd=td,capture_output=True,timeout=10)
                    assert not list(Path(td).iterdir()), 'unexpected raw usage write'
                    outputs.append((r.returncode,r.stdout,r.stderr))
            assert outputs[0]==outputs[1],(command,tail,outputs)
            assert outputs[0][0]==2 and not outputs[0][1]
    print('30 additional POSIX raw group usage cases passed; no writes')
else:
    print('POSIX raw argv cases not executed on this host')
