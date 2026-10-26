#!/usr/bin/env python3
"""Owned opaque datapack world mutation fixtures only; no acquisition or archive parsing."""
from pathlib import Path
import os, json, subprocess, tempfile, hashlib, argparse, sys, errno, contextlib
if os.name != 'nt':
    import fcntl

def native_bytes(p):
    return str(p).encode('utf-8', 'surrogatepass') if os.name == 'nt' else os.fsencode(p)

# Do not let Windows cleanup clear the shared readonly file attribute.
# Retain owned fixtures there and report their location, including on failure.
@contextlib.contextmanager
def owned_temp():
    if os.name == 'nt':
        td = tempfile.mkdtemp(prefix='owned-datapack-mutate-world-')
        try:
            yield td
        finally:
            print('RETAINED owned readonly fixture directory:', td)
    else:
        with tempfile.TemporaryDirectory(prefix='owned-datapack-mutate-world-') as td:
            yield td
violations = []

def require(condition, detail=None):
    if not condition:
        violations.append(detail)
parser = argparse.ArgumentParser(description=__doc__)
for key in ['zig', 'rust', 'oracle-source']:
    parser.add_argument('--' + key, required=True)
parser.add_argument('--output')
args = parser.parse_args()
assert subprocess.check_output(['git', '-C', args.oracle_source, 'rev-parse', 'HEAD'], text=True).strip() == '5a4310debbcee26b3d2391bab2339f79cd6ec52e'
BINS = {'zig': Path(args.zig).resolve(), 'rust': Path(args.rust).resolve()}
PAYLOAD = b'owned opaque payload only'
HASH = hashlib.sha1(PAYLOAD).hexdigest()

def snap(root):
    out = {}
    for p in [root, *root.rglob('*')]:
        s = p.lstat()
        out[str(p.relative_to(root))] = (p.read_bytes().hex() if p.is_file() else None, s.st_dev, s.st_ino, s.st_mode, s.st_mtime_ns, s.st_nlink)
    return out
rows = []
variants = ['string', 'array', 'native_array', 'missing_file_string', 'missing_file_array', 'blocked_string', 'blocked_array', 'empty_string', 'empty_array', 'bad_selector_string', 'bad_selector_array', 'blocked_bad_selector_string', 'blocked_bad_selector_array', 'overflow', 'fraction', 'integral', 'quoted_fraction', 'nul', 'no_selector_array', 'both_selectors_array']
with owned_temp() as td:
    native_name = 'game-\ud800' if os.name == 'nt' else os.fsdecode(b'game-\xff')
    probe = Path(td) / native_name
    raw_capable = True
    try:
        probe.mkdir()
        probe.rmdir()
    except OSError as e:
        if os.name != 'nt' and e.errno == errno.EILSEQ or (os.name == 'nt' and getattr(e, 'winerror', None) == 123):
            raw_capable = False
            print('CAPABILITY raw physical directory rejected:', repr(e))
        else:
            raise
    if not raw_capable:
        variants.remove('native_array')
    for family, kind in [('datapacks', 'datapack')]:
        cases = variants + (['missing_world_string', 'missing_world_array', 'bad_world_string', 'bad_world_array', 'locked_string', 'locked_array', 'locked_bad_selector_string', 'locked_bad_selector_array', 'locked_both_selectors_string', 'locked_both_selectors_array'] if family == 'datapacks' else [])
        if os.name == 'nt':
            cases = [case for case in cases if not case.startswith('locked')]
        for op in ['enable', 'disable', 'remove']:
            for case in cases:
                row = {'method': family + '.' + op, 'case': case, 'diagnostics': {}}
                for engine, exe in BINS.items():
                    t = Path(td) / str(len(rows)) / engine
                    t.mkdir(parents=True)
                    game = t / 'game'
                    world_name = native_name if case == 'native_array' else 'world'
                    game.mkdir()
                    home = game / 'saves' / world_name
                    (home / family).mkdir(parents=True)
                    (home / '.jmcl').mkdir()
                    file = family + ('/a.jar' if family == 'mods' else '/a.zip')
                    enabled = op != 'enable'
                    path = home / (file + ('' if enabled else '.disabled'))
                    path.write_bytes(PAYLOAD)
                    peer = t / 'peer'
                    os.link(path, peer)
                    if case.startswith('missing_file'):
                        path.unlink()
                    peer.chmod(0o444)
                    (t / 'sibling').write_bytes(b'preserve sibling')
                    (t / 'blocked').write_bytes(b'not directory')
                    entry = {'kind': kind, 'file': file, 'sha1': HASH, 'size': len(PAYLOAD), 'source': {'provider': 'manual', 'project_id': None, 'version_id': None, 'slug': None}, 'enabled': enabled, 'as_dependency': False}
                    registry = home / '.jmcl/content.json'
                    registry.write_text(json.dumps({'schema_version': 1, 'entries': [entry]}))
                    directory = str(t / 'blocked' if case.startswith('blocked') else game)
                    d = list(native_bytes(world_name)) if 'array' in case or case in ['overflow', 'fraction', 'integral', 'quoted_fraction', 'nul'] else world_name
                    if case.startswith('empty'):
                        d = [] if 'array' in case else ''
                    if case == 'overflow':
                        d = [256]
                    if case == 'nul':
                        d = [0]
                    params = {'directory': directory, 'world': d, 'file': '../bad' if 'bad_selector' in case else file}
                    if case == 'no_selector_array':
                        params.pop('file')
                    if 'both_selectors' in case:
                        params['project'] = 'project'
                    if case.startswith('missing_world') or case.startswith('bad_world'):
                        name = 'missing' if case.startswith('missing_world') else 'bad/name'
                        params['world'] = list(name.encode()) if 'array' in case else name
                    lock = None
                    if case.startswith('locked'):
                        lock = open(home / 'session.lock', 'wb')
                        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    responses = []
                    states = []
                    before = snap(t)
                    peerbefore = peer.stat()
                    row['diagnostics'][engine] = []
                    for step in range(2):
                        req = json.dumps({'protocol': 1, 'id': 't', 'method': row['method'], 'params': params})
                        if case in ['fraction', 'integral', 'quoted_fraction']:
                            first = native_bytes(world_name)[0]
                            replacement = str(first) + ('.5' if case == 'fraction' else '.0' if case == 'integral' else '.0000000000000001')
                            if case == 'quoted_fraction':
                                replacement = json.dumps(replacement)
                            req = req.replace('"world": [' + str(first) + ',', '"world": [' + replacement + ',', 1)
                        pre = snap(t)
                        o = subprocess.run([str(exe), 'rpc'], input=(req + '\n').encode(), capture_output=True, check=True, timeout=15)
                        require(not o.stderr, (row, engine, step, 'not o.stderr'))
                        response = json.loads(o.stdout)
                        responses.append(response)
                        if engine == 'zig' and case in ['string', 'array', 'native_array', 'missing_file_string', 'missing_file_array', 'integral']:
                            require(response['event'] == ('error' if op == 'remove' and step == 1 else 'result'), (row, engine, step, "response['event'] == ('error' if op == 'remove' and step == 1 else 'result')"))
                            if op == 'remove' and step == 1:
                                require(response.get('error', {}).get('code') == 'CONTENT_NOT_FOUND', (row, engine, step, "response.get('error', {}).get('code') == 'CONTENT_NOT_FOUND'"))
                        ps = peer.stat()
                        row['diagnostics'][engine].append({
                            'step': step,
                            'peer_before': {'device': peerbefore.st_dev, 'inode': peerbefore.st_ino, 'mode': peerbefore.st_mode, 'mtime_ns': peerbefore.st_mtime_ns, 'links': peerbefore.st_nlink, 'bytes_hex': PAYLOAD.hex()},
                            'peer_after': {'device': ps.st_dev, 'inode': ps.st_ino, 'mode': ps.st_mode, 'mtime_ns': ps.st_mtime_ns, 'links': ps.st_nlink, 'bytes_hex': peer.read_bytes().hex()},
                        })
                        require(ps.st_mode & 0o222 == 0, (row, engine, step, 'peer remains readonly'))
                        require(peer.read_bytes() == PAYLOAD and (ps.st_dev, ps.st_ino, ps.st_mode, ps.st_mtime_ns) == (peerbefore.st_dev, peerbefore.st_ino, peerbefore.st_mode, peerbefore.st_mtime_ns), (row, engine, step, 'peer.read_bytes() == PAYLOAD and (ps.st_dev, ps.st_ino, ps.st_mode, ps.st_mtime_ns) == (peerbefore.st_dev, peerbefore.st_ino, peerbefore.st_mode, peerbefore.st_mtime_ns)'))
                        require((t / 'sibling').read_bytes() == b'preserve sibling', (row, engine, step, "(t / 'sibling').read_bytes() == b'preserve sibling'"))
                        reg = json.loads(registry.read_text())
                        normal = (home / file).exists()
                        disabled = (home / (file + '.disabled')).exists()
                        if response['event'] == 'error':
                            require(snap(t) == pre, (row, engine, step, 'snap(t) == pre'))
                        else:
                            raw_world = native_bytes(world_name)
                            try: expected_world = raw_world.decode('utf-8')
                            except UnicodeDecodeError: expected_world = list(raw_world)
                            require(response['result']['world'] == expected_world, (row, engine, step, 'exact world String/byte output'))
                            require(response['event'] == 'result', (row, engine, step, "response['event'] == 'result'"))
                            if op == 'remove':
                                require(reg['entries'] == [] and (not normal) and (not disabled) and (ps.st_nlink == 1), (row, engine, step, "reg['entries'] == [] and (not normal) and (not disabled) and (ps.st_nlink == 1)"))
                            else:
                                expected = dict(entry, enabled=op == 'enable')
                                require(reg['entries'] == [expected], (row, engine, step, "reg['entries'] == [expected]"))
                                missing = case.startswith('missing_file')
                                require(normal == (op == 'enable' and (not missing)) and disabled == (op == 'disable' and (not missing)), (row, engine, step, "normal == (op == 'enable' and (not missing)) and disabled == (op == 'disable' and (not missing))"))
                                require(response['result']['changed'] == (step == 0), (row, engine, step, "response['result']['changed'] == (step == 0)"))
                                if not missing:
                                    s = (home / (file + ('' if op == 'enable' else '.disabled'))).stat()
                                    require((s.st_dev, s.st_ino, s.st_mode) == (ps.st_dev, ps.st_ino, ps.st_mode) and ps.st_nlink == 2, (row, engine, step, '(s.st_dev, s.st_ino, s.st_mode) == (ps.st_dev, ps.st_ino, ps.st_mode) and ps.st_nlink == 2'))
                        states.append({'registry': reg, 'normal': normal, 'disabled': disabled, 'peer_links': ps.st_nlink})
                    row[engine] = {'responses': responses, 'states': states}
                    if lock:
                        lock.close()
                rows.append(row)
if args.output:
    Path(args.output).write_text(json.dumps(rows, indent=2))
diffs = [r for r in rows if r['zig'] != r['rust']]
print(len(rows), 'paired sequences;', len(rows) * 2, 'RPC pairs;', len(diffs), 'differing sequences')
for r in diffs:
    print('MISMATCH:', json.dumps(r, sort_keys=True))
if violations:
    print('INVARIANT FAILURES:', json.dumps(violations, indent=2))
assert not violations, f'{len(violations)} direct state/peer/source contract failures'
assert not diffs, f'{len(diffs)} source/Rust sequence mismatches'
