#!/usr/bin/env python3
"""Owned local CLI paths only: no acquisition, adoption, or executable fixtures."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import sys
import subprocess
import tempfile

p = argparse.ArgumentParser()
for key in ('zig', 'rust', 'oracle-source'):
    p.add_argument('--' + key, required=True)
a = p.parse_args()
binaries = [Path(a.zig).resolve(), Path(a.rust).resolve()]
assert subprocess.check_output(['git', '-C', a.oracle_source, 'rev-parse', 'HEAD'], text=True).strip() == '5a4310debbcee26b3d2391bab2339f79cd6ec52e'
print('binary SHA256:', {str(b): hashlib.sha256(b.read_bytes()).hexdigest() for b in binaries})
FAMILIES = {'mods': ('mod', '.jar'), 'resourcepacks': ('resourcepack', '.zip'), 'shaderpacks': ('shaderpack', '.zip'), 'datapacks': ('datapack', '.zip')}
checks = 0

def native_bytes(value):
    text = os.fspath(value)
    return text.encode('utf-8', 'surrogatepass') if os.name == 'nt' else os.fsencode(text)


def native_raw_name(raw):
    if os.name != 'nt':
        return os.fsdecode(raw)
    # CreateProcessW and native paths receive actual unpaired WCHARs, not a
    # lossy UTF-8 spelling. Snapshot bytes use the matching WTF-8 representation.
    return raw.decode('latin1').replace('\xff', '\ud800').replace('\xfe', '\ud801').replace('\xfd', '\ud802')


def snapshot(root):
    result = {}
    for parent, dirs, files in os.walk(root):
        for name in dirs + files:
            q = parent / name if isinstance(parent, Path) else Path(parent) / name
            s = q.lstat()
            result[native_bytes(q.relative_to(root)).hex()] = {
                'mode': stat.S_IMODE(s.st_mode), 'nlink': s.st_nlink,
                'data': q.read_bytes().hex() if stat.S_ISREG(s.st_mode) else None,
                'link': native_bytes(os.readlink(q)).hex() if stat.S_ISLNK(s.st_mode) else None,
                'ino': s.st_ino, 'mtime': s.st_mtime_ns,
            }
    return result

def comparable(s):
    return {k: {f: v for f, v in e.items() if f not in ('ino', 'mtime')} for k, e in s.items()}

@contextmanager
def owned_fixture():
    # Failed fixtures are retained, including their readonly hardlinks. Cleanup
    # is only for a completed successful engine run, never to recover a failure.
    state = {'path': tempfile.mkdtemp(prefix='owned-local-cli-path-'), 'keep': True}
    try:
        yield state
    finally:
        if not state['keep'] and os.name != 'nt':
            shutil.rmtree(state['path'])
        elif sys.exc_info()[0] is not None:
            print('retained failed fixture: ' + state['path'], flush=True)


def execute(binary, family, raw, case, operations):
    with owned_fixture() as fixture:
        td = fixture['path']
        root = Path(td)
        directory = native_raw_name(b'root\xff') if raw in ('directory', 'both') else 'root中文'
        world = native_raw_name(b'World\xfe') if raw in ('world', 'both') else 'World中文'
        home = root / directory / 'demo'
        game = home / '.minecraft'
        content = game / 'saves' / world if family == 'datapacks' else game
        if case == 'missing-saves' and family == 'datapacks':
            # Construct absence; never delete a readonly linked fixture to get it.
            game.mkdir(parents=True)
            content = root / 'unused-content-fixture'
        (content / '.jmcl').mkdir(parents=True)
        (content / family).mkdir()
        (home / 'instance.json').write_bytes(b'{"id":"demo","name":"Demo","version_id":"demo"}')
        peer = root / 'peer'
        peer.write_bytes(b'inert local bytes')
        peer.chmod(0o444)
        target_name = ('target' + FAMILIES[family][1])
        if case == 'raw-file':
            target_name = native_raw_name(b'target\xfd') + FAMILIES[family][1]
        file = family + '/' + target_name
        target = content / file
        os.link(peer, target)
        os.link(peer, root / 'sibling')
        entry = {'kind': FAMILIES[family][0], 'file': file, 'sha1': hashlib.sha1(peer.read_bytes()).hexdigest(), 'size': len(peer.read_bytes()), 'source': {'provider': 'manual', 'project_id': None, 'version_id': None, 'slug': None}, 'enabled': True, 'as_dependency': False}
        if case == 'raw-file':
            entry['file'] = list(native_bytes(file))
        registry = content / '.jmcl/content.json'
        registry.write_text(json.dumps({'schema_version': 1, 'entries': [entry]}), encoding='utf-8')
        if case == 'bad-manifest': (home / 'instance.json').write_bytes(b'{')
        if case == 'no-manifest': (home / 'instance.json').unlink()
        if case == 'missing-world': world = 'Absent'
        if case == 'bad-world': world = '../bad'
        if case == 'missing-root': directory = native_raw_name(b'missing\xff') if raw == 'directory' else 'missing'
        if case == 'root-file':
            directory = native_raw_name(b'blocked\xff') if raw == 'directory' else 'blocked'
            (root / directory).write_bytes(b'not a directory')
        if case == 'root-alias':
            (root / 'alias').symlink_to(directory, target_is_directory=True)
            directory = 'alias'
        if case == 'world-alias':
            (game / 'saves' / 'Alias').symlink_to(world, target_is_directory=True)
            world = 'Alias'
        lock = None
        if case == 'locked':
            import fcntl
            lock = (content / 'session.lock').open('w+b')
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        history, diagnostics, failures = [], [], []
        for operation_index, op in enumerate(operations):
            argv = [str(binary), 'instance', family, op, 'demo', '--instances-directory', './' + directory]
            if case == 'repeat-directory':
                argv[6] = './ignored-missing-root'
                argv += ['--instances-directory', './' + directory]
            if case == 'empty-directory': argv += ['--instances-directory', '']
            if family == 'datapacks' and case != 'missing-world-option': argv += ['--world', world]
            if family == 'datapacks' and case == 'repeat-world':
                argv[-1] = '../ignored-invalid'
                argv += ['--world', world]
            if family == 'datapacks' and case == 'empty-world': argv += ['--world', '']
            if op != 'list': argv += ['--file', file]
            if case == 'selector-conflict': argv += ['--project', 'abc']
            if case == 'missing-option-value': argv += ['--instances-directory']
            if case == 'unknown-option': argv += ['--unknown', 'value']
            if case == 'json' and op == 'list': argv += ['--json']
            before = snapshot(root)
            result = subprocess.run(argv, cwd=root, capture_output=True, timeout=15)
            after = snapshot(root)
            diagnostics.append({'operation': op, 'status': result.returncode,
                                'stdout_hex': result.stdout.hex(), 'stderr_hex': result.stderr.hex(),
                                'before': before, 'after': after})
            try:
                for name in ('peer', 'sibling'):
                    key = name.encode().hex()
                    for field in ('data', 'mode', 'ino', 'mtime'):
                        assert before[key][field] == after[key][field], (family, raw, case, op, name, field)
                    assert after[key]['nlink'] == before[key]['nlink'] - (1 if op == 'remove' and result.returncode == 0 else 0)
                if case in ('normal', 'json', 'raw-file', 'repeat-directory', 'repeat-world', 'world-alias') or family == 'datapacks' and case in ('bad-manifest', 'no-manifest'):
                    assert result.returncode == 0, (binary, family, raw, case, op, result.stdout, result.stderr)
                    assert result.stderr == b''
                    if op == 'disable':
                        assert result.stdout == b'disabled\t' + native_bytes(file) + b'\n'
                        assert not target.exists() and Path(str(target) + '.disabled').is_file()
                        assert json.loads(registry.read_bytes())['entries'][0]['enabled'] is False
                    if op == 'enable':
                        assert result.stdout == b'enabled\t' + native_bytes(file) + b'\n'
                        assert target.is_file() and not Path(str(target) + '.disabled').exists()
                        assert json.loads(registry.read_bytes())['entries'][0]['enabled'] is True
                    if op == 'remove':
                        assert result.stdout == b'removed\t' + native_bytes(file) + b'\t' + entry['sha1'].encode() + b'\n'
                        assert not target.exists() and json.loads(registry.read_bytes())['entries'] == []
                if case == 'locked':
                    assert result.returncode == (0 if op == 'list' else 1)
                    if op != 'list':
                        assert result.stderr == b'the world is in use by a running game\n'
                        assert before == after
                if case == 'missing-root':
                    created = set(after) - set(before)
                    expected = [directory] if family != 'datapacks' else [directory, directory + '/demo', directory + '/demo/.minecraft']
                    assert created == ({native_bytes(Path(x)).hex() for x in expected} if operation_index == 0 else set())
                    assert all(before[k] == after[k] for k in before), (family, raw, 'existing entries changed')
                if case in ('missing-option-value', 'unknown-option', 'selector-conflict', 'empty-world', 'missing-world-option'):
                    assert before == after, (family, raw, case, 'argument failure mutated disk')
                if op == 'list' and case != 'missing-root': assert before == after, (family, raw, case, 'list mutated disk')
            except (AssertionError, OSError, ValueError, KeyError, IndexError) as error:
                failures.append({'operation': op, 'error': repr(error)})
            history.append((result.returncode, result.stdout.hex(), result.stderr.hex(), comparable(after)))
            if failures:
                break  # Do not perform more mutations after a broken invariant.
        if lock:
            lock.close()
        fixture['keep'] = bool(failures) or os.name == 'nt'
        return {'history': history, 'diagnostics': diagnostics, 'failures': failures,
                'retained_fixture': fixture['path'] if fixture['keep'] else None}

raw_supported = False
with tempfile.TemporaryDirectory(prefix='owned-cli-capability-') as td:
    try:
        Path(td, native_raw_name(b'raw\xff')).mkdir()
        raw_supported = True
    except OSError as e:
        import errno
        expected = e.errno == errno.EILSEQ if os.name == 'posix' else getattr(e, 'winerror', None) == 123
        if not expected:
            raise
        print('raw-name success unavailable: ' + ('filesystem EILSEQ' if os.name == 'posix' else 'ERROR_INVALID_NAME') + '; Unicode and exact rejection controls required')
if os.name == 'nt':
    print('Windows raw mode uses physical lone-surrogate WCHAR directory/world/file arguments; owned readonly fixture directories are retained, never chmod-cleaned')

if not raw_supported:
    for family in FAMILIES:
        for op in ('list', 'disable'):
            results = []
            for binary in binaries:
                try:
                    with owned_fixture() as rejection_fixture:
                        root = Path(rejection_fixture['path'])
                        (root / 'peer').write_bytes(b'inert')
                        (root / 'peer').chmod(0o444)
                        before = snapshot(root)
                        argv = [str(binary), 'instance', family, op, 'demo', '--instances-directory', native_raw_name(b'raw\xff')]
                        if family == 'datapacks': argv += ['--world', 'World']
                        if op != 'list': argv += ['--file', family + '/absent.zip']
                        r = subprocess.run(argv, cwd=root, capture_output=True, timeout=15)
                        after = snapshot(root)
                        valid = r.returncode == 1 and after == before
                        rejection_fixture['keep'] = os.name == 'nt' or not valid
                        results.append({'valid': valid, 'wire': (r.returncode, r.stdout.hex(), r.stderr.hex()),
                                        'before': before, 'after': after, 'fixture': rejection_fixture['path']})
                except Exception as error:
                    results.append({'valid': False, 'error': repr(error), 'engine': str(binary)})
            if not all(r['valid'] for r in results) or results[0]['wire'] != results[1]['wire']:
                print('LOCAL_CLI_REJECTION_FAILURE ' + json.dumps({'family': family, 'operation': op, 'reports': results}), flush=True)
                raise AssertionError((family, op, 'strict rejection comparison failed'))
            checks += 1
    print('PASS: 8 exact filesystem raw-directory rejection controls; not raw-name success')

for family in FAMILIES:
    modes = ['unicode'] + (['directory'] + (['world', 'both'] if family == 'datapacks' else []) if raw_supported else [])
    for raw in modes:
        cases = ['normal', 'json', 'repeat-directory', 'bad-manifest', 'no-manifest', 'missing-root', 'root-file', 'empty-directory', 'missing-option-value', 'unknown-option', 'selector-conflict']
        if raw_supported: cases += ['raw-file']
        if family == 'datapacks': cases += ['missing-saves', 'missing-world', 'bad-world', 'missing-world-option', 'repeat-world', 'empty-world']
        if os.name == 'posix':
            cases += ['root-alias']
            if family == 'datapacks': cases += ['world-alias', 'locked']
        for case in cases:
            ops = ['list', 'disable', 'enable', 'remove'] if case in ('normal', 'raw-file', 'repeat-directory', 'repeat-world', 'world-alias') else ['list', 'disable']
            reports = []
            for binary in binaries:
                try:
                    reports.append(execute(binary, family, raw, case, ops))
                except Exception as error:
                    # Always collect the other engine, even for fixture or
                    # subprocess errors; these never become acceptance.
                    reports.append({'failures': [{'error': repr(error)}], 'engine': str(binary)})
            if any(report['failures'] for report in reports) or reports[0]['history'] != reports[1]['history']:
                print('LOCAL_CLI_CASE_FAILURE ' + json.dumps({'family': family, 'raw': raw,
                      'case': case, 'engines': [str(b) for b in binaries], 'reports': reports}), flush=True)
                raise AssertionError((family, raw, case, 'strict per-engine contract or differential failure'))
            checks += len(ops)
print(f'PASS: {checks} exact local content CLI path/guard/output/disk comparisons; no acquisition')
