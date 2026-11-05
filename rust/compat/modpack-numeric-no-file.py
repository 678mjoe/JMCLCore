#!/usr/bin/env python3
"""No --file is ever supplied: parsing must stop before provider/archive/filesystem."""
import argparse
from pathlib import Path
import subprocess
import tempfile
p=argparse.ArgumentParser();p.add_argument('--zig',required=True);p.add_argument('--rust',required=True);a=p.parse_args()
binaries=[str(Path(x).resolve()) for x in [a.zig,a.rust]]
cases=[]
for flag in ['--workers','--retries']:
    for value in ['1','+1','1__0','0_1','-0','+_1','-_0','-+0','1_','256','-1','33','6','0','+0_1','0_5','-0_0','18446744073709551616']:
        cases.append([flag,value])
    cases += [[flag,'+_1',flag,'1'],[flag,'256',flag,'1'],[flag,'0',flag,'1']]
for args in cases:
    results=[]
    for binary in binaries:
        with tempfile.TemporaryDirectory(prefix='owned-modpack-numeric-') as td:
            r=subprocess.run([binary,'instance','modpack','install','demo']+args,cwd=td,capture_output=True,timeout=10)
            assert not list(Path(td).iterdir())
            results.append((r.returncode,r.stdout,r.stderr))
    assert results[0]==results[1],(args,results)
    assert results[0][0] in [1,2] and not results[0][1]
print(f'{len(cases)} no-file numeric cases match exact status/stdout/stderr; no writes')
