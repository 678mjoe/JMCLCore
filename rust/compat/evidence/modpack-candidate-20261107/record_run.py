#!/usr/bin/env python3
"""Run only two source-reviewed synthetic modpack service tests."""
import hashlib, json, os, pathlib, re, subprocess
ROOT = pathlib.Path('/workspace/scratch/6fda792bd6ec/JMCLCore-acceptance-20261107')
BASE = pathlib.Path('/workspace/scratch/6fda792bd6ec/jmcl-tools')
OUT = pathlib.Path('/workspace/shared/jmcl-modpack-candidate-20261107')
TMP = BASE / 'modpack-candidate-owned-20261107'
EXPECTED_COMMIT = '13c7044c0d095f17b904d0113e0b29d6a8a50ef9'
EXPECTED_TREE = 'e3479126c4665ec2975af43a7f0f7f2da85c2df3'
TESTS = ['hardlink_content_private_overrides_and_cache_reuse',
         'rollback_preserves_verified_cache_and_retry_has_no_new_format']
SOURCES = ['rust/tests/modpack_install.rs', 'rust/src/modpack/service.rs',
           'rust/src/modpack/planning.rs', 'rust/src/modpack/schema.rs',
           'rust/src/install/mod.rs',
           'rust/Cargo.toml', 'rust/Cargo.lock']
def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args], text=True).strip()
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
assert git('rev-parse', 'HEAD') == EXPECTED_COMMIT
assert git('rev-parse', 'HEAD^{tree}') == EXPECTED_TREE
assert not git('status', '--porcelain')
OUT.mkdir(exist_ok=True)
TMP.mkdir(exist_ok=True)
assert not list(TMP.iterdir()), 'Owned temp directory must start empty'
toolchain = BASE / 'rustup-home/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin'
env = os.environ.copy()
env.update(CARGO_HOME=str(BASE / 'cargo-home'), RUSTUP_HOME=str(BASE / 'rustup-home'),
           CARGO_TARGET_DIR=str(BASE / 'acceptance-matrix-target-20261107'),
           CARGO_NET_OFFLINE='true', CARGO_INCREMENTAL='0',
           CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
           RUSTC=str(toolchain / 'rustc'), TMPDIR=str(TMP))
env['PATH'] = str(toolchain) + os.pathsep + env['PATH']
manifest = {'tested_commit': EXPECTED_COMMIT, 'tested_tree': EXPECTED_TREE,
            'corresponding_remote_commit': 'b98f0b13bbd677f8034d9d8b4d8e3c4ddb90b30c',
            'platform': 'Linux x86_64 GNU', 'toolchain': 'Rust/Cargo 1.99.1',
            'source_fingerprints': {p: sha(ROOT / p) for p in SOURCES},
            'cargo_offline_locked': True, 'tmpdir': str(TMP), 'results': [],
            'scope': 'Owned generated mrpack ZIP only; ordinary hello/private bytes; injected Fake transport; no HTTP client, external request, Java, processor or game.',
            'production_test_fixtures_dependencies_workflows_unchanged': True}
for index, name in enumerate(TESTS, 1):
    command = ['cargo', 'test', '--manifest-path', 'rust/Cargo.toml', '--offline', '--locked',
               '--test', 'modpack_install', name, '--', '--exact', '--nocapture', '--test-threads=1']
    log = OUT / f'test-{index:02}.log'
    result = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
    log.write_bytes(json.dumps({'command': command}).encode() + b'\n' + result.stdout +
                    json.dumps({'returncode': result.returncode}).encode() + b'\n')
    output = result.stdout.decode('utf-8')
    assert result.returncode == 0, (name, result.returncode, str(log))
    assert re.search(r'^running 1 test$', output, re.M)
    assert re.search(r'^test ' + re.escape(name) + r' \.\.\. ok$', output, re.M)
    matches = re.findall(r'^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; (\d+) filtered out;', output, re.M)
    assert len(matches) == 1, (name, 'nonzero exact result missing')
    assert not list(TMP.iterdir()), ('temporary remnants', str(TMP))
    manifest['results'].append({'name': name, 'command': command, 'passed': 1, 'failed': 0,
                                'ignored': 0, 'filtered_out': int(matches[0]),
                                'log': log.name, 'sha256': sha(log), 'owned_tmp_empty': True})
    print(f'passed {name}; exact 1/0/0, no owned temporary remnants', flush=True)
assert git('rev-parse', 'HEAD') == EXPECTED_COMMIT and not git('status', '--porcelain')
manifest['clean_tree_before_and_after'] = True
manifest['test_binary_sha256'] = sha(BASE / 'acceptance-matrix-target-20261107/debug/deps/modpack_install-099eb49f1803088e')
(OUT / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(str(OUT / 'manifest.json'), flush=True)
