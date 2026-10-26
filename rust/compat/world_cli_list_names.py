#!/usr/bin/env python3
"""Instance CLI list-only enumeration. No cleanup candidates or archive operations."""
import argparse
import errno
import gzip
import hashlib
import json
import os
from pathlib import Path
import stat
import struct
import subprocess
import tempfile

p = argparse.ArgumentParser()
for key in ('zig', 'rust', 'oracle-source'):
    p.add_argument('--' + key, required=True)
a = p.parse_args()
assert subprocess.check_output(['git', '-C', a.oracle_source, 'rev-parse', 'HEAD'], text=True).strip() == '5a4310debbcee26b3d2391bab2339f79cd6ec52e'
binaries = [Path(a.zig).resolve(), Path(a.rust).resolve()]
print('binary SHA256:', {str(b): hashlib.sha256(b.read_bytes()).hexdigest() for b in binaries})
def text(s):
    b = s.encode()
    return struct.pack('>H', len(b)) + b

def tag(t, n, v):
    return bytes([t]) + text(n) + v

nbt = tag(10, '', tag(10, 'Data', tag(8, 'LevelName', text('Synthetic')) + tag(3, 'GameType', struct.pack('>i', 1)) + b'\0') + b'\0')
level = gzip.compress(nbt, mtime=0)
def native_bytes(name):
    return name.encode('utf-8', 'surrogatepass') if os.name == 'nt' else os.fsencode(name)

def raw_name(n):
    return '\ud800' + n if os.name == 'nt' else os.fsdecode(b'\xff') + n

def wire_bytes(name):
    return name.encode() if isinstance(name, str) else bytes(name)

def snapshot(root):
    result = {}
    for q in [root, *root.rglob('*')]:
        s = q.lstat()
        result[native_bytes(q.relative_to(root).as_posix()).hex()] = {
            'data_hex': q.read_bytes().hex() if q.is_file() else None,
            'mode': stat.S_IMODE(s.st_mode), 'ino': s.st_ino,
            'mtime': s.st_mtime_ns, 'nlink': s.st_nlink,
        }
    return result

root = Path(tempfile.mkdtemp(prefix='owned-world-list-names-'))
# Retain fixtures unchanged on every platform; no cleanup chmod or directory
# rewriting obscures failure evidence.
probe = root / raw_name('Probe')
raw_supported = True
try:
    probe.mkdir()
except OSError as error:
    if not (getattr(error, 'winerror', None) == 123 if os.name == 'nt' else error.errno == errno.EILSEQ):
        raise
    raw_supported = False
    print('physical opaque-name success unavailable; Unicode enumeration/count controls remain mandatory')

cases = [('unicode-mixed', None, False), ('unicode-limit-1024', 1024, False), ('unicode-limit-1025', 1025, False)]
if raw_supported:
    cases += [('opaque-mixed', None, True), ('opaque-limit-1024', 1023, True), ('opaque-limit-1025', 1024, True)]
checks = 0
for label, amount, opaque in cases:
    directory = root / label
    instance = directory / 'demo'
    saves = instance / '.minecraft' / 'saves'
    saves.mkdir(parents=True)
    (instance/'instance.json').write_text(json.dumps({'schema_version':1,'id':'demo','name':'Synthetic','version_id':'1.20','source':'official'}))
    names = (['Alpha', 'Zulu', '世界', 'é', '😀', 'MissingMeta', 'MalformedMeta'] if amount is None
             else [f'World{i:04d}' for i in range(amount)])
    if opaque:
        names += [raw_name('A'), raw_name('B')] if amount is None else [raw_name('Limit')]
    expected = sorted(native_bytes(n) for n in names)
    for name in names:
        world = saves / name
        world.mkdir()
        if amount is None and name != 'MissingMeta':
            (world / 'level.dat').write_bytes(b'not-nbt' if name == 'MalformedMeta' else level)
    if amount is None:
        (saves / '.hidden').mkdir()
        (saves / 'ordinary-file').write_bytes(b'inert')
        if os.name == 'posix':
            for name in ('trailing.', 'trailing ', 'bad\x01'):
                (saves / name).mkdir()
            (saves / 'Alias').symlink_to('Alpha', target_is_directory=True)
    assert not any('.jmcl-copy-' in q.name or '.jmcl-trash-' in q.name for q in directory.rglob('*'))
    for json_mode in [False,True]:
        before = snapshot(root)
        reports = []
        args = ['instance','worlds','list','demo','--instances-directory',str(directory)]+(['--json'] if json_mode else [])
        for binary in binaries:
            try:
                result = subprocess.run([str(binary), *args], capture_output=True, timeout=45)
                after = snapshot(root)
                record = {'status': result.returncode, 'stdout_hex': result.stdout.hex(), 'stderr_hex': result.stderr.hex(), 'before': before, 'after': after}
                reports.append(record)
                failures = []
                try:
                    assert after == before
                    if len(expected)>1024:
                        assert result.returncode==1 and result.stdout==b'' and result.stderr==b'failed to list world: TooManyWorlds\n'
                    else:
                        assert result.returncode==0 and result.stderr==b''
                        if json_mode:
                            worlds=json.loads(result.stdout)['worlds']
                            assert [wire_bytes(w['name']) for w in worlds]==expected
                            for world in worlds:
                                malformed=wire_bytes(world['name']) in (b'MissingMeta',b'MalformedMeta') or amount is not None
                                assert world['level_name']==(None if malformed else 'Synthetic')
                        else:
                            rows=[]
                            for name in expected:
                                missing=name in (b'MissingMeta',b'MalformedMeta') or amount is not None
                                size=0 if amount is not None or name==b'MissingMeta' else len(b'not-nbt') if name==b'MalformedMeta' else len(level)
                                rows.append(name+b'\t'+(b'-' if missing else b'Synthetic')+b'\t-\t'+(b'-' if missing else b'creative')+b'\t'+str(size).encode()+b'\tidle\tunknown\n')
                            assert result.stdout==b''.join(rows)
                except (AssertionError, KeyError, ValueError) as error:
                    failures.append(repr(error))
                record['failures'] = failures
            except Exception as error:
                reports.append({'failures': [repr(error)], 'engine': str(binary)})
        if any(r['failures'] for r in reports) or reports[0]['stdout_hex'] != reports[1]['stdout_hex']:
            print('WORLD_LIST_NAMES_FAILURE ' + json.dumps({'case': label, 'json': json_mode, 'fixture': str(root), 'expected_names_hex': [n.hex() for n in expected], 'reports': reports}), flush=True)
            raise AssertionError((label, 'strict enumeration contract mismatch'))
        checks += 1
print(f'PASS: {checks} exact instance CLI enumeration/count commands; metadata fallbacks and complete disk preservation checked')
