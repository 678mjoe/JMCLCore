#!/usr/bin/env python3
"""Exact pinned-source, exact-name pure tests. Never run the complete test suite."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

SOURCE_COMMIT = '190be14a4c78b4ef0d59b0ae43713b09fbe67c5f'
SOURCE_TREE = 'facf839b53db4027a15afca12064d41c916ec422'
RUST_VERSION = '1.99.1'
ALLOWLIST = Path(__file__).with_name('bounded_native_allowlist.json')
ALLOWLIST_SHA256 = '2ee1bd23991b7cfd9867d8ea1259453d13ddc8d354352f1bef1fada2ecd15da2'


def verified_allowlist(data):
    if hashlib.sha256(data).hexdigest() != ALLOWLIST_SHA256:
        raise ValueError("Reviewed allowlist changed")
    return json.loads(data)


def names_for(platform, groups):
    if platform not in ('linux', 'darwin', 'win32'):
        raise ValueError('Only the reviewed Linux, macOS and Windows hosts are supported')
    names = groups['common'] + (groups['windows'] if platform == 'win32' else groups['unix'])
    if platform == 'linux':
        names += groups['linux']
    if len(names) != {'linux': 25, 'darwin': 23, 'win32': 20}[platform] or len(set(names)) != len(names):
        raise ValueError('Unexpected or duplicate reviewed test names')
    if any(not re.fullmatch(r'[A-Za-z0-9_:]+', n) for n in names):
        raise ValueError('Invalid exact test name')
    return names


def validate_inventory(output, names):
    present = [line[:-6] for line in output.splitlines() if line.endswith(': test')]
    if any(present.count(name) != 1 for name in names):
        raise ValueError('A required test is absent or duplicated; zero-test success is forbidden')


def validate_result(returncode, output, name):
    if returncode != 0 or not re.search(r'\b1 passed; 0 failed; 0 ignored;', output):
        raise ValueError(f'Expected one non-ignored passing test: {name}')
    if f'test {name} ... ok' not in output:
        raise ValueError(f'Missing exact passing test identity: {name}')


def checked(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True, **kwargs).stdout.strip()


def verify_source(source):
    def git(*args):
        return checked(['git', '-C', str(source), *args])
    if git('rev-parse', 'HEAD') != SOURCE_COMMIT or git('rev-parse', 'HEAD^{tree}') != SOURCE_TREE:
        raise ValueError('Source revision/tree is not the reviewed immutable checkpoint')
    if git('status', '--porcelain', '--untracked-files=all'):
        raise ValueError('Source checkout must be clean, including untracked files')
    for tool in ('cargo', 'rustc'):
        version = checked([tool, '--version'])
        if not version.startswith(f'{tool} {RUST_VERSION} '):
            raise ValueError(f'Unexpected compiler/toolchain: {version}')


def report_after_verification(source, report):
    verify_source(source)
    print(json.dumps(report, indent=2))


def build_library(command):
    built = subprocess.run(command, stdout=subprocess.PIPE, text=True)
    # Cargo JSON contains compiler diagnostics on stdout, including on failure.
    for line in built.stdout.splitlines():
        row = json.loads(line)
        if row.get("reason") == "compiler-message":
            rendered = row.get("message", {}).get("rendered")
            if rendered:
                print(rendered, file=sys.stderr, end="")
    built.check_returncode()
    return built


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--source', required=True, type=Path)
    p.add_argument('--offline', action='store_true')
    args = p.parse_args()
    source = args.source.resolve()
    verify_source(source)
    groups = verified_allowlist(ALLOWLIST.read_bytes())
    names = names_for(sys.platform, groups)
    # An owned build target is kept outside the immutable source checkout.
    with tempfile.TemporaryDirectory(prefix='jmcl-pure-native-') as build:
        command = ['cargo', 'test', '--manifest-path', str(source / 'rust/Cargo.toml'),
                   '--locked', '--lib', '--no-run', '--message-format=json', '--target-dir', build]
        if args.offline:
            command.append('--offline')
        built = build_library(command)
        artifacts = []
        for line in built.stdout.splitlines():
            row = json.loads(line)
            if (row.get('reason') == 'compiler-artifact' and row.get('profile', {}).get('test')
                    and row.get('target', {}).get('kind') == ['lib'] and row.get('executable')):
                artifacts.append(Path(row['executable']).resolve())
        if len(artifacts) != 1 or not artifacts[0].is_relative_to(Path(build).resolve()):
            raise ValueError('Expected exactly one owned library test executable')
        binary = artifacts[0]
        validate_inventory(checked([str(binary), '--list', '--format', 'terse']), names)
        for name in names:
            result = subprocess.run([str(binary), name, '--exact', '--nocapture'],
                                    cwd=build, capture_output=True, text=True, timeout=30)
            validate_result(result.returncode, result.stdout + result.stderr, name)
        report = {'source_commit': SOURCE_COMMIT, 'source_tree': SOURCE_TREE,
                          'platform': sys.platform, 'rust_version': RUST_VERSION,
                          'test_binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                          'passed': len(names), 'ignored': 0, 'tests': names}
    report_after_verification(source, report)


if __name__ == '__main__':
    main()
