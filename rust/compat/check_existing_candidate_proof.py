#!/usr/bin/env python3
"""Digest-bound source review for a test-only proof, not a Rust call-graph analyzer."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = Path(__file__).with_name('existing-candidate-proof-guard.json')
EXPECTED = json.loads(MANIFEST.read_text())
MANAGED = 'rust/src/java/managed.rs'
PROOF = 'rust/src/java/existing_candidate.rs'
MODULE = b'#[cfg(test)]\n#[path = "existing_candidate.rs"]\nmod existing_candidate;\n'

def digest(data):
    return hashlib.sha256(data.replace(b'\r\n', b'\n')).hexdigest()

def require(ok, message):
    if not ok:
        raise ValueError(message)

def sources():
    return {name: (ROOT / name).read_bytes()
            for name in [*EXPECTED['protected'], MANAGED, PROOF]}

def validate(data):
    for name, expected in EXPECTED['protected'].items():
        require(digest(data[name]) == expected, 'Reviewed dependency changed: ' + name)
    managed = data[MANAGED].replace(b'\r\n', b'\n')
    require(managed.count(MODULE) == 1, 'Proof must remain exactly one cfg(test) module')
    require(digest(managed.replace(MODULE, b'')) == EXPECTED['managed_without_test_module'],
            'Operational managed implementation changed')
    require(digest(data[PROOF]) == EXPECTED['proof_sha256'],
            'Proof changed: repeat pure dependency review')

if __name__ == '__main__':
    validate(sources())
    print('PASS: reviewed buffer proof is test-only; operational managed and protected dependencies unchanged')
