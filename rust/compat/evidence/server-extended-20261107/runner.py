#!/usr/bin/env python3
"""Two exact, reviewed Linux handshake and loopback fixture tests. No automatic process kill."""
import hashlib, json, os, pathlib, re, subprocess, sys, time
ROOT = pathlib.Path('/workspace/scratch/6fda792bd6ec/JMCLCore-acceptance-20261107')
TARGET = pathlib.Path('/workspace/scratch/6fda792bd6ec/jmcl-tools/acceptance-matrix-target-20261107/debug')
BINARY = TARGET / 'deps/server_lifecycle-bb2ee2f25e6f3a99'
OUT = pathlib.Path('/workspace/shared/jmcl-rust-server-lifecycle-extended-20261107.log')
TMP = pathlib.Path('/workspace/scratch/6fda792bd6ec/jmcl-tools/server-lifecycle-extended-owned-20261107')
EXPECTED = {
    '/usr/lib/jvm/default-java/bin/java',
    '/usr/lib/jvm/java-1.21.0-openjdk-amd64/bin/java',
    '/usr/lib/jvm/java-21-openjdk-amd64/bin/java',
}
TESTS = ['supervisor_handshake_failure_kills_child_and_cleans_state', 'managed_rcon_and_query_use_property_derived_loopback_ports']
EXPECTED_CHILDREN = [0, 1]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args], text=True).strip()

assert not git('status', '--porcelain'), 'Commit reviewed sources before execution'
assert pathlib.Path('/usr/local/sbin').is_dir()
assert not (pathlib.Path('/usr/local/sbin') / 'java').exists()
candidates = set()
for base in ('/usr/lib/jvm', '/usr/java'):
    base = pathlib.Path(base)
    if base.is_dir():
        for entry in base.iterdir():
            path = entry / 'bin/java'
            if path.is_file() and os.access(path, os.X_OK):
                candidates.add(str(path))
assert candidates == EXPECTED, ('system candidate inventory changed', sorted(candidates))
release = pathlib.Path('/usr/lib/jvm/java-21-openjdk-amd64/release').read_text()
assert re.search(r'^JAVA_VERSION="21\.', release, re.M)
assert {str(pathlib.Path(p).resolve()) for p in candidates} == {
    '/usr/lib/jvm/java-21-openjdk-amd64/bin/java'
}, 'System aliases must resolve to the reviewed installed JVM, not merely agree'
TMP.mkdir(exist_ok=True)
assert not list(TMP.iterdir()), 'Owned temp directory must start empty'
env = {'PATH': '/usr/local/sbin', 'LANG': 'C', 'LC_ALL': 'C', 'TZ': 'UTC',
       'TMPDIR': str(TMP), 'JMCL_TEST_SERVER_FIXTURE_MAJOR': '99'}
header = {'commit': git('rev-parse', 'HEAD'), 'tree': git('rev-parse', 'HEAD^{tree}'),
          'binary': str(BINARY), 'binary_sha256': sha(BINARY),
          'core_sha256': sha(TARGET / 'jmcl-core-rust'), 'env': env,
          'system_candidates': [{'path': p, 'resolved': str(pathlib.Path(p).resolve()),
                                 'sha256': sha(pathlib.Path(p))} for p in sorted(candidates)],
          'release_sha256': sha(pathlib.Path('/usr/lib/jvm/java-21-openjdk-amd64/release')),
          'scope': 'Handshake failure uses owned absolute fixture only. RCON/Query executes installed JVM property/version probes, owned major99 shell server, and self-owned TCP/UDP endpoints on 127.0.0.1 only. No download/install/archive or real Minecraft server.'}
with OUT.open('w') as log:
    log.write(json.dumps(header, ensure_ascii=True, sort_keys=True) + '\n')
    log.flush()
    for name, expected_children in zip(TESTS, EXPECTED_CHILDREN):
        command = [str(BINARY), name, '--exact', '--nocapture', '--test-threads=1']
        log.write(json.dumps({'command': command}) + '\n')
        log.flush()
        start = log.tell()
        child = subprocess.Popen(command, cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
                                 stdout=log, stderr=subprocess.STDOUT)
        print(f'running {name}, test pid={child.pid}, evidence={OUT}', flush=True)
        # Do not timeout-kill: a detached supervisor needs owned identity cleanup.
        while True:
            try:
                code = child.wait(timeout=60)
                break
            except subprocess.TimeoutExpired:
                print(f'PENDING: {name} pid={child.pid}; inspect evidence and owned TMPDIR. No process killed.', flush=True)
        text = OUT.read_text()[start:]
        assert code == 0, (name, 'failed', code)
        assert 'running 1 test' in text and '1 passed; 0 failed; 0 ignored;' in text
        assert 'fixture cleanup failed' not in text
        owned = re.findall(r'owned fixture running: pid=(\d+) supervisor=(\d+) executable=(\S+)', text)
        assert len(owned) == expected_children, (name, 'unexpected observed fixture executions', owned)
        assert 'owned process scan: no live fixture arguments for ' in text
        failed_supervisor = re.findall(r'owned failing supervisor: pid=(\d+) root=(\S+)', text)
        assert len(failed_supervisor) == (1 if expected_children == 0 else 0)
        for supervisor, root in failed_supervisor:
            owned.append((supervisor, supervisor, root + '/java-fixture'))
        deadline = time.monotonic() + 5
        while True:
            leftovers = []
            for pid, supervisor, executable in owned:
                owned_root = str(pathlib.Path(executable).parent).encode()
                for kind, number in [('child', pid), ('supervisor', supervisor)]:
                    try:
                        cmdline = pathlib.Path(f'/proc/{number}/cmdline').read_bytes()
                    except (FileNotFoundError, ProcessLookupError):
                        continue
                    if owned_root in cmdline:
                        leftovers.append((kind, number, cmdline.decode(errors='replace')))
            if not leftovers or time.monotonic() >= deadline:
                break
            time.sleep(.05)
        assert not leftovers, ('owned process remains; preserve evidence and stop second test', leftovers)
        assert not list(TMP.iterdir()), ('owned temporary root not clean', str(TMP))
        log.write(json.dumps({'verified': name, 'passed': 1, 'failed': 0, 'ignored': 0,
                              'observed_fixture_children': expected_children, 'failed_supervisors': len(failed_supervisor), 'live_owned_processes_after': 0,
                              'owned_tmp_entries_after': 0}) + '\n')
        log.flush()
        print(f'passed {name}; {expected_children} positively observed fixture child, {len(failed_supervisor)} failing supervisor, no live owned child/supervisor or temporary remnants', flush=True)
print(json.dumps({'log': str(OUT), 'sha256': sha(OUT)}), flush=True)
