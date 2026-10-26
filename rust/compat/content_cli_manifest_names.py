#!/usr/bin/env python3
"""Local CLI discarded-manifest validation. Owned inert files, no acquisition."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile

p = argparse.ArgumentParser()
for option in ('zig', 'rust', 'oracle-source'):
    p.add_argument('--' + option, required=True)
a = p.parse_args()
assert subprocess.check_output(['git', '-C', a.oracle_source, 'rev-parse', 'HEAD'], text=True).strip() == '5a4310debbcee26b3d2391bab2339f79cd6ec52e'
binaries = [Path(a.zig).resolve(), Path(a.rust).resolve()]
print('binary SHA256:', {str(b): hashlib.sha256(b.read_bytes()).hexdigest() for b in binaries})
families = {'mods': ('mod', '.jar'), 'resourcepacks': ('resourcepack', '.zip'), 'shaderpacks': ('shaderpack', '.zip'), 'datapacks': ('datapack', '.zip')}
cases = []
for label, token, error in [
    ('string', '"Demo"', None), ('unicode-string', json.dumps('世界'), None),
    ('ascii-array', '[68,101,109,111]', None), ('unicode-array', json.dumps(list('世界'.encode())), None),
    ('opaque-array', '[255,65]', None), ('control-array', '[255,0,10]', None),
    ('typed-array', '["+6__5",66.0,6.7e1]', None), ('max-array', json.dumps([65] * 256), None),
    ('empty-string', '""', 'InvalidInstanceName'), ('empty-array', '[]', 'InvalidInstanceName'),
    ('oversize', json.dumps([65] * 257), 'InvalidInstanceName'),
    ('fraction', '[65.5]', 'InvalidNumber'), ('precision', '[1.0000000000000001]', 'InvalidNumber'),
    ('overflow', '[256]', 'Overflow'), ('null', 'null', 'UnexpectedToken'),
]:
    cases.append((label, '{"id":"demo","name":' + token + ',"version_id":"demo"}', error))
cases += [
    ('stored-invalid', '{"id":"../bad","name":[],"version_id":""}', 'InvalidInstanceId'),
    ('stored-mismatch', '{"id":"other","name":[65],"version_id":"demo"}', 'InstanceIdMismatch'),
    ('schema-before-name', '{"schema_version":2,"id":"demo","name":[],"version_id":""}', 'UnsupportedInstanceSchema'),
    ('source-before-name', '{"id":"demo","name":[],"version_id":"","source":"bad"}', 'InvalidEnumTag'),
    ('duplicate', '{"id":"demo","name":[65],"name":[66],"version_id":"demo"}', 'DuplicateField'),
    ('malformed', '{', 'UnexpectedEndOfInput'),
]

def snapshot(root):
    result = {}
    for q in root.rglob('*'):
        s = q.lstat()
        result[q.relative_to(root).as_posix()] = {
            'data_hex': q.read_bytes().hex() if q.is_file() else None,
            'mode': stat.S_IMODE(s.st_mode), 'ino': s.st_ino,
            'mtime': s.st_mtime_ns, 'nlink': s.st_nlink,
        }
    return result

def comparable(tree):
    return {k: {f: v for f, v in e.items() if f not in ('ino', 'mtime')} for k, e in tree.items()}

def execute(binary, family, manifest, expected_error):
    root = Path(tempfile.mkdtemp(prefix='owned-cli-manifest-name-'))
    # Failed and Windows fixtures are retained without clearing readonly bits.
    history, diagnostics, failures = [], [], []
    home = root / 'instances/demo'
    game = home / '.minecraft'
    content = game / 'saves/World' if family == 'datapacks' else game
    (content / '.jmcl').mkdir(parents=True)
    (content / family).mkdir()
    metadata = home / 'instance.json'
    metadata.write_bytes(manifest.encode())
    peer = root / 'peer'
    peer.write_bytes(b'inert local metadata fixture')
    peer.chmod(0o444)
    file = family + '/target' + families[family][1]
    target = content / file
    os.link(peer, target)
    os.link(peer, root / 'sibling')
    sha1 = hashlib.sha1(peer.read_bytes()).hexdigest()
    entry = {'kind': families[family][0], 'file': file, 'sha1': sha1, 'size': len(peer.read_bytes()), 'source': {'provider': 'manual', 'project_id': None, 'version_id': None, 'slug': None}, 'enabled': True, 'as_dependency': False}
    registry = content / '.jmcl/content.json'
    registry.write_text(json.dumps({'schema_version': 1, 'entries': [entry]}))
    for op in ('list', 'disable', 'enable', 'remove'):
        argv = [str(binary), 'instance', family, op, 'demo', '--instances-directory', str(root / 'instances')]
        if family == 'datapacks': argv += ['--world', 'World']
        if op != 'list': argv += ['--file', file]
        before = snapshot(root)
        r = subprocess.run(argv, capture_output=True, timeout=15)
        after = snapshot(root)
        diagnostics.append({'operation': op, 'status': r.returncode, 'stdout_hex': r.stdout.hex(), 'stderr_hex': r.stderr.hex(), 'before': before, 'after': after})
        try:
            assert before['instances/demo/instance.json'] == after['instances/demo/instance.json']
            for name in ('peer', 'sibling'):
                for field in ('data_hex', 'mode', 'ino', 'mtime'):
                    assert before[name][field] == after[name][field], (name, field)
                assert after[name]['nlink'] == before[name]['nlink'] - (1 if op == 'remove' and r.returncode == 0 else 0)
            if expected_error is not None:
                assert (r.returncode, r.stdout, r.stderr) == (1, b'', ('failed to load instance: ' + expected_error + '\n').encode())
                assert before == after
            else:
                assert r.returncode == 0 and r.stderr == b''
                entries = json.loads(registry.read_bytes())['entries']
                if op == 'list':
                    assert before == after
                    expected = f'{families[family][0]}\t{file}\t{sha1}\t{entry["size"]}\tmanual\t-\tenabled\texplicit\n'.encode()
                    assert r.stdout == expected
                elif op == 'remove':
                    assert r.stdout == f'removed\t{file}\t{sha1}\n'.encode()
                    assert entries == [] and not target.exists()
                else:
                    assert r.stdout == (('disabled' if op == 'disable' else 'enabled') + '\t' + file + '\n').encode()
                    assert len(entries) == 1 and entries[0]['enabled'] is (op == 'enable')
                    assert target.exists() is (op == 'enable')
                    assert Path(str(target) + '.disabled').exists() is (op == 'disable')
            history.append((r.returncode, r.stdout.hex(), r.stderr.hex(), comparable(after)))
        except (AssertionError, OSError, ValueError, KeyError, IndexError) as error:
            failures.append({'operation': op, 'error': repr(error)})
            break
    if not failures and os.name != 'nt':
        shutil.rmtree(root)
    return {'history': history, 'diagnostics': diagnostics, 'failures': failures,
            'retained_fixture': str(root) if failures or os.name == 'nt' else None}

count = 0
for family in families:
    for label, manifest, error in cases:
        expected_error = None if family == 'datapacks' else error
        reports = []
        for binary in binaries:
            try:
                reports.append(execute(binary, family, manifest, expected_error))
            except Exception as failure:
                reports.append({'failures': [repr(failure)], 'engine': str(binary)})
        if any(r['failures'] for r in reports) or reports[0]['history'] != reports[1]['history']:
            print('CONTENT_CLI_MANIFEST_FAILURE ' + json.dumps({'family': family, 'case': label, 'manifest': manifest, 'expected_error': expected_error, 'engines': [str(b) for b in binaries], 'reports': reports}), flush=True)
            raise AssertionError((family, label, 'strict contract or differential mismatch'))
        count += 4
print(f'PASS: {count} exact manifest-name CLI comparisons; datapack bypass, positive transitions and immutable peers checked')
