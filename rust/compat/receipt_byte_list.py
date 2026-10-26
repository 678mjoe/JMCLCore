#!/usr/bin/env python3
"""Owned runtime.list metadata only: no executable files or staging directories."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

BASE = dict(schema_version=1, provider='mojang', id='fixture', platform='linux',
            major_version=17, java_home='', executable='bin/absent', version='17',
            vendor='Synthetic', component='fixture', package_uuid='uuid',
            package_name='package', source_url='https://example.invalid/metadata-only',
            files_total=1, bytes_total=7, installed_at_ms=123)
FIELDS = ['id', 'platform', 'java_home', 'executable', 'version', 'vendor',
          'component', 'package_uuid', 'package_name', 'source_url']

def raw_field(field, token):
    d = {k: v for k, v in BASE.items() if k != field}
    return json.dumps(d)[:-1] + ',' + json.dumps(field) + ':' + token + '}'


def snapshot(root):
    result = {}
    for p in [root, *root.rglob('*')]:
        st = p.lstat()
        data = os.readlink(p) if p.is_symlink() else p.read_bytes() if p.is_file() else None
        result[str(p.relative_to(root))] = (data, st.st_ino, st.st_mode, st.st_mtime_ns, st.st_nlink)
    return result


def main():
    parser = argparse.ArgumentParser()
    for key in ['zig', 'rust', 'oracle-source']:
        parser.add_argument('--' + key, required=True)
    a = parser.parse_args()
    assert subprocess.check_output(['git', '-C', a.oracle_source, 'rev-parse', 'HEAD'], text=True).strip() == '5a4310debbcee26b3d2391bab2339f79cd6ec52e'
    binaries = [Path(a.zig).resolve(), Path(a.rust).resolve()]
    print('binary SHA256:', {str(b): hashlib.sha256(b.read_bytes()).hexdigest() for b in binaries})
    cases = [('ordinary', json.dumps(BASE), 1)]
    for field in FIELDS:
        cases.append((field + '-ascii-array', raw_field(field, json.dumps(list(BASE[field].encode()))), 1))
    for field in ['version', 'vendor', 'component', 'package_uuid', 'package_name']:
        for token in ['[255,9,10,0]', '[]', 'null', '""', '"中文\\t\\n\\u0000"']:
            cases.append((field + token, raw_field(field, token), 1))
    for field, value in [('executable', b'bin/\xff'), ('java_home', b'\xff'), ('source_url', b'https://example.invalid/\xff')]:
        cases.append((field + '-opaque', raw_field(field, json.dumps(list(value))), 1))
    for token in ['0', '1', '"0"', '"1"', '"+1"', '"0__1"', '"-00"', '"-0_0"', '"zulu"']:
        cases.append(('provider-' + token, raw_field('provider', token), 1))
    for token in ['-0', '1.0', '"1.0"', '2', '-1', 'true', 'null', '[]', '[109,111,106,97,110,103]', '"other"']:
        cases.append(('provider-invalid-' + token, raw_field('provider', token), 0))
    for token, count in [('[65]', 1), ('[65.0]', 1), ('["6_5"]', 1), ('["65.0"]', 1),
                         ('[65.0000000000000001]', 0), ('["65.0000000000000001"]', 0),
                         ('[256]', 0), ('[-1]', 0), ('[true]', 0), ('[null]', 0), ('[[65]]', 0),
                         ('{}', 0), ('false', 0), ('65', 0)]:
        cases.append(('typed-byte-' + token, raw_field('vendor', token), count))
    for field, token in [('id', '[255]'), ('id', '[]'), ('platform', '[255]'), ('executable', '[0]'),
                         ('executable', '[]'), ('executable', '[47,120]'), ('executable', '[97,92,98]'),
                         ('executable', '[46,46,47,97]'), ('java_home', '[97,58,98]'), ('source_url', '[255]'),
                         ('major_version', '0'), ('major_version', '17.0000000000000001'),
                         ('installed_at_ms', '123.0000000000000001')]:
        cases.append(('invalid-' + field + token, raw_field(field, token), 0))
    for field, token in [('id', json.dumps('a' * 64)), ('executable', json.dumps('a' * 1024)),
                         ('source_url', json.dumps('https://' + 'a' * 2040))]:
        cases.append(('limit-' + field, raw_field(field, token), 1))
    for field, token in [('id', json.dumps('a' * 65)), ('executable', json.dumps('a' * 1025)),
                         ('source_url', json.dumps('https://' + 'a' * 2041)), ('vendor', json.dumps('a' * 16384))]:
        cases.append(('over-limit-' + field, raw_field(field, token), 0))
    cases += [('unknown-large', json.dumps(BASE)[:-1] + ',"ignored":1e400}', 1),
              ('duplicate', json.dumps(BASE)[:-1] + ',"vendor":"second"}', 0),
              ('missing-provider', json.dumps({k:v for k,v in BASE.items() if k != 'provider'}), 0),
              ('outer-array', '[' + json.dumps(BASE) + ']', 0), ('truncated', '{', 0)]
    for label, token, count in [
        ('raw-invalid-utf8', b'"\xff"', 0),
        ('nested-high-surrogate', b'{"nested":["\\ud800"]}', 0),
        ('nested-low-surrogate', b'{"nested":["\\udc00"]}', 0),
        ('nested-paired-surrogate', b'{"nested":["\\ud800\\udc00"]}', 1),
        ('escaped-literal-backslash-u', b'"\\\\ud800"', 1),
        ('unknown-huge-number', b'1e400', 1),
    ]:
        cases.append((label, json.dumps(BASE).encode()[:-1] + b',"ignored":' + token + b'}', count))
    comparisons = 0
    with tempfile.TemporaryDirectory(prefix='owned-receipt-list-') as td:
        root = Path(td)
        (root/'sibling').write_bytes(b'untouched sibling')
        os.link(root/'sibling', root/'sibling-link')
        def check(store, label, count=None, names=None, mutable=False):
            nonlocal comparisons
            before = snapshot(root)
            assert not any('.staging-' in n for n in before)
            wire = json.dumps({'protocol':1, 'id':'receipt', 'method':'java.runtime.list', 'params':{'store_directory':str(store)}}).encode() + b'\n'
            commands = [(['rpc'], wire), (['java','runtime','list','--store',str(store),'--json'], None),
                        (['java','runtime','list','--store',str(store)], None)]
            for index, (args, data) in enumerate(commands):
                outputs = []
                for binary in binaries:
                    p = subprocess.run([str(binary), *args], input=data, capture_output=True, timeout=30, cwd=root)
                    outputs.append((p.returncode, p.stdout, p.stderr))
                    if not mutable:
                        assert snapshot(root) == before, (label, binary, 'disk changed')
                    else:
                        after = snapshot(root)
                        created = {str(store.relative_to(root)), str((store/'runtimes').relative_to(root))}
                        assert set(after) - set(before) <= created
                        for key in before:
                            if key != '.':
                                assert after[key] == before[key], (label, key, 'preexisting entry changed')
                        assert store.is_dir() and (store/'runtimes').is_dir()
                        assert not list((store/'runtimes').iterdir())
                    if count is None:
                        if index == 0:
                            assert p.returncode == 0 and not p.stderr
                            assert json.loads(p.stdout)['error'] == {'code':'JAVA_RUNTIME_ERROR','message':'Managed runtime operation failed'}
                        else:
                            assert p.returncode == 1 and not p.stdout
                            assert p.stderr == b'failed to list managed runtimes: TooManyRuntimes\n'
                    if count is not None:
                        assert p.returncode == 0 and not p.stderr, (label, binary, p)
                        if index < 2:
                            runtimes = json.loads(p.stdout)['result']['runtimes']
                            assert len(runtimes) == count, (label, binary, runtimes)
                            if names is not None:
                                assert [r['name'] for r in runtimes] == names
                assert outputs[0] == outputs[1], (label, index, outputs)
                comparisons += 1
        for index, (label, raw, count) in enumerate(cases):
            store = root/str(index)
            directory = store/'runtimes/receipt-fixture'
            directory.mkdir(parents=True)
            f = directory/'.jmcl-runtime.json'
            f.write_bytes(raw.encode() if isinstance(raw, str) else raw)
            os.link(f, store/'receipt-peer')
            # Only metadata files exist; executable receipt text is never resolved.
            assert list(directory.iterdir()) == [f]
            check(store, label, count)
        store = root/'sorting'
        for name in ['zulu-last', 'A-first', 'middle', '.hidden', 'invalid name']:
            d = store/'runtimes'/name; d.mkdir(parents=True)
            (d/'.jmcl-runtime.json').write_text(json.dumps(BASE))
        check(store, 'sorting-and-directory-name', 3, ['A-first','middle','zulu-last'])
        if os.name != 'nt':
            alias = store/'runtimes/receipt-alias'; alias.mkdir()
            (alias/'.jmcl-runtime.json').symlink_to(store/'runtimes/middle/.jmcl-runtime.json')
            (store/'runtimes/directory-alias').symlink_to(store/'runtimes/middle', target_is_directory=True)
            check(store, 'receipt-alias-and-directory-alias', 4, ['A-first','middle','receipt-alias','zulu-last'])
        else:
            print('Receipt symlink success not exercised on Windows: no privilege assumption; ordinary hardlink identity still checked.')
        for n in [256,257]:
            store = root/('count-'+str(n))
            for i in range(n):
                d = store/'runtimes'/('receipt-%03d' % i); d.mkdir(parents=True)
                (d/'.jmcl-runtime.json').write_text(json.dumps(BASE))
            check(store, 'count-'+str(n), 256 if n==256 else None)
        store = root/'missing'
        # Both list implementations create root/runtimes; assert exactly that effect.
        check(store, 'missing-root', 0, mutable=True)
        assert store.is_dir() and (store/'runtimes').is_dir() and not list((store/'runtimes').iterdir())
    print(f'PASS: {comparisons} exact runtime.list RPC/CLI comparisons; {len(cases)} receipt cases; no executables or staging directories')

if __name__ == '__main__':
    main()
