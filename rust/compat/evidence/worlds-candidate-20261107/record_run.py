#!/usr/bin/env python3
"""Execute the reviewed exact client-world slice; never run a broad test filter."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess

ROOT = Path('/workspace/scratch/6fda792bd6ec/JMCLCore-worlds-candidate-20261107')
RUN = ROOT.parent / 'jmcl-tools/worlds-candidate-20261107'
LOGS = RUN / 'logs'
CANDIDATE = '009b046ac0a4916c0fa5b073212cc7404db2e51a'
TREE = '5e61790fde937ab927b2b79d1a59d3323f2dbaa3'
PINNED = '5a4310debbcee26b3d2391bab2339f79cd6ec52e'
ZIG = RUN / 'zig-out/bin/jmcl-core'
RUST = ROOT.parent / 'jmcl-tools/worlds-candidate-target-20261107/debug/jmcl-core-rust'
ORDINARY = [
    'worlds::tests::metadata_and_rename_preserve_unknown_nbt',
    'worlds::tests::mutable_duplicates_are_copies_not_hardlinks',
    'worlds::tests::locked_restore_refuses_and_stale_lock_allows',
    'worlds::tests::export_excludes_registry_lock_and_temporary_files',
    'worlds::tests::ordinary_zip_roundtrip_preserves_files',
]
ORACLE = [
    'worlds::tests::zig_rust_metadata_and_archive_interoperability',
    'worlds::tests::zig_rust_world_cli_mutation_results_match',
]

def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()

def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()

def check_sources():
    assert git(ROOT, 'rev-parse', 'HEAD') == CANDIDATE
    assert git(ROOT, 'rev-parse', 'HEAD^{tree}') == TREE
    assert not git(ROOT, 'status', '--porcelain', '--untracked-files=all')
    assert git(ROOT.parent / 'JMCLCore-oracle', 'rev-parse', 'HEAD') == PINNED
    assert not git(ROOT.parent / 'JMCLCore-oracle', 'status', '--porcelain', '--untracked-files=all')
    assert subprocess.check_output(['rustc', '--version'], text=True).startswith('rustc 1.99.1 ')
    assert subprocess.check_output(['cargo', '--version'], text=True).startswith('cargo 1.99.1 ')

def run(cmd, name, env=None):
    result = subprocess.run(cmd, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=120)
    with (LOGS / name).open('wb') as log:
        log.write(('COMMAND ' + json.dumps(cmd) + '\n').encode())
        log.write(result.stdout)
        log.write(('\nRETURN_CODE ' + str(result.returncode) + '\n').encode())
    if result.returncode:
        print(result.stdout.decode(errors='replace'))
    assert result.returncode == 0, (name, result.returncode)
    return result.stdout.decode()

check_sources()
prefix = ['cargo', 'test', '--manifest-path', str(ROOT / 'rust/Cargo.toml'), '--offline', '--locked', '--lib']
env = os.environ.copy()
env['JMCL_ZIG_ORACLE'] = str(ZIG)
selected = []
for i, name in enumerate(ORDINARY + ORACLE):
    ignored = name in ORACLE
    cmd = prefix + [name, '--', '--exact', '--nocapture'] + (['--ignored'] if ignored else [])
    output = run(cmd, f'test-{i+1:02d}.log', env)
    assert len(re.findall(r'^running 1 test$', output, re.M)) == 1, (name, output)
    assert len(re.findall(r'^test result: ok\. 1 passed; 0 failed; 0 ignored;', output, re.M)) == 1, (name, output)
    assert f'test {name} ... ok' in output, (name, output)
    selected.append({'name': name, 'explicit_ignored_opt_in': ignored, 'passed': 1, 'failed': 0, 'ignored': 0, 'command': cmd, 'log': f'test-{i+1:02d}.log'})
    print('PASS', name, flush=True)
cmd = ['python3', str(ROOT / 'rust/compat/world_cli_archives.py'), '--zig', str(ZIG), '--zig-sha256', digest(ZIG), '--rust', str(RUST), '--rust-sha256', digest(RUST)]
output = run(cmd, 'cli-archives.log', env)
assert output.strip() == 'world CLI archive corpus: 9 paired scenarios passed; 48 command results checked', output
check_sources()
report = {
    'candidate_commit': CANDIDATE, 'candidate_tree': TREE,
    'oracle_commit': PINNED, 'oracle_tree': git(ROOT.parent/'JMCLCore-oracle', 'rev-parse', 'HEAD^{tree}'),
    'oracle_sha256': digest(ZIG), 'rust_sha256': digest(RUST),
    'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
    'cargo': subprocess.check_output(['cargo', '--version'], text=True).strip(),
    'platform': subprocess.check_output(['uname', '-srm'], text=True).strip(),
    'cargo_home': os.environ.get('CARGO_HOME'), 'cargo_target_dir': os.environ.get('CARGO_TARGET_DIR'),
    'tmpdir': os.environ.get('TMPDIR'),
    'selected_tests': selected, 'passed': 7, 'failed': 0, 'ignored': 0,
    'cli_archive_corpus': {'paired_scenarios': 9, 'command_results': 48, 'command': cmd, 'log': 'cli-archives.log'},
    'source_fingerprints': {str(p.relative_to(ROOT)): digest(p) for p in sorted((ROOT/'rust/src/worlds').glob('*.rs'))} | {'rust/compat/world_cli_archives.py': digest(ROOT/'rust/compat/world_cli_archives.py'), 'rust/Cargo.lock': digest(ROOT/'rust/Cargo.lock')},
    'logs_sha256': {p.name: digest(p) for p in sorted(LOGS.iterdir())},
}
(RUN/'manifest.json').write_text(json.dumps(report, indent=2)+'\n')
print('PASS world slice: 7 exact tests; CLI corpus 9 paired scenarios / 48 command results', flush=True)
