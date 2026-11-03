#!/usr/bin/env python3
"""Offline invalid-parameter oracle corpus; never requests metadata or installs files."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--zig', required=True)
parser.add_argument('--rust', required=True)
args = parser.parse_args()
binaries = [pathlib.Path(p).resolve() for p in (args.zig, args.rust)]
cases = [None, {}, {'id': ''}, {'id': []}, {'id': 'v', 'directory': []},
         {'id': [255], 'directory': ''}, {'id': [256]}, {'id': [-1]},
         {'id': [False]}, {'id': 'v', 'directory': None},
         {'id': '', 'source': None}, {'id': '', 'os': 'invalid'},
         {'id': '', 'arch': 99}, {'id': '', 'fabric_loader': False},
         {'id': '', 'workers': None, 'java_paths': False, 'java_policy': None,
          'retries': False, 'store_directory': None},
         {'id': [], 'source': 'bmclapi', 'os': None, 'arch': None}]
for i, params in enumerate(cases):
    body = {'protocol': 1, 'id': str(i), 'method': 'install.prepare', 'params': params}
    wire = (json.dumps(body, separators=(',', ':')) + '\n').encode()
    results = []
    for binary in binaries:
        with tempfile.TemporaryDirectory(prefix='jmcl-prepare-schema-') as root:
            p = subprocess.run([str(binary), 'rpc'], input=wire, capture_output=True,
                               cwd=root, timeout=10, check=False)
            assert not list(pathlib.Path(root).iterdir()), (i, binary, 'unexpected disk changes')
            results.append((p.returncode, p.stdout, p.stderr))
    assert results[0] == results[1], (i, body, results)
    assert json.loads(results[0][1])['error']['code'] == 'INVALID_PARAMS', (i, results)
print(json.dumps({'cases_passed': len(cases), 'platform': 'local only',
                  'scope': 'invalid parameters, exact exit/stdout/stderr and empty disk',
                  'binary_sha256': {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in binaries}}, indent=2))
