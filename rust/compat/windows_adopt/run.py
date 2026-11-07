"""One exact Windows Root::adopt identity test, with original pipe-byte evidence."""
import base64
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import struct
import subprocess
import tempfile

TARGET = 'x86_64-pc-windows-msvc'
TEST = 'adoption_reuses_real_file_identity_and_manual_registry'
MARKER = b'WINDOWS_ADOPT_IDENTITY_OK original=source=cas after_first_and_second_adopt'
BASELINE = 'bdd6cb3e55d69cb9e0e02037d251635e289207a8'
SOURCE_TREE = '99bfa7577af7597ab8069aecd278bb4e9a352296'
TEST_SHA256 = '28fb816e464c205e2fdf021b9b5ab74d0f80c3aadc310ffbbe9dccf3035abe0c'

def require(ok, message):
    if not ok:
        raise RuntimeError(message)

def validate_result(code, raw):
    require(code == 0, 'test command failed')
    require(raw.count(MARKER) == 1, 'Windows identity assertions not observed exactly once')
    require(len(re.findall(rb'test result: ok\. 1 passed; 0 failed; 0 ignored;', raw)) == 1, 'expected 1/0/0')
    require(b'running 1 test' in raw and TEST.encode() in raw, 'wrong test selection')

def validate_pe(data):
    require(len(data) >= 64 and data[:2] == b'MZ', 'not PE')
    offset = struct.unpack_from('<I', data, 60)[0]
    require(offset + 26 <= len(data), 'truncated PE')
    require(data[offset:offset+4] == b'PE\0\0', 'bad PE signature')
    require(struct.unpack_from('<H', data, offset+4)[0] == 0x8664, 'not x64')
    flags = struct.unpack_from('<H', data, offset+22)[0]
    require(flags & 2 and not flags & 0x2000, 'not executable')
    require(struct.unpack_from('<H', data, offset+24)[0] == 0x20b, 'not PE32+')

def execute(command, root, env, runner=subprocess.run):
    timed_out = False
    try:
        result = runner(command, cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=1200)
        code, raw = result.returncode, result.stdout
    except subprocess.TimeoutExpired as exc:
        code, raw, timed_out = None, exc.output or b'', True
    print('RAW_COMMAND '+json.dumps(dict(command=command, returncode=code, timed_out=timed_out, raw_bytes=len(raw), raw_sha256=hashlib.sha256(raw).hexdigest(), raw_base64=base64.b64encode(raw).decode())), flush=True)
    return code, raw

def main():
    root = Path(__file__).resolve().parents[3]
    def git(*args):
        return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()
    require(platform.system() == 'Windows' and platform.machine() == 'AMD64', 'native Windows x64 required')
    rust = subprocess.check_output(['rustc', '-Vv'], text=True)
    require('host: '+TARGET in rust and 'release: 1.99.1\n' in rust, 'wrong Rust host/version')
    require(not git('status', '--porcelain'), 'dirty checkout')
    require(git('rev-parse', 'HEAD:rust/src') == SOURCE_TREE, 'production Rust changed')
    require(git('rev-parse', BASELINE+'^{tree}') == '0e8e901bc587cf3c6d7dc1ba78821c4701785e20', 'baseline mismatch')
    changed = git('diff', '--name-only', BASELINE, 'HEAD').splitlines()
    require(all(n == 'rust/tests/content_adopt.rs' or n == '.github/workflows/windows-adopt.yml' or n.startswith('rust/compat/windows_adopt/') for n in changed), 'unreviewed change')
    require(hashlib.sha256((root/'rust/tests/content_adopt.rs').read_bytes()).hexdigest() == TEST_SHA256, 'test pin mismatch')
    owned = Path(tempfile.mkdtemp(prefix='jmcl-windows-adopt-', dir=os.environ['RUNNER_TEMP']))
    env = {k:v for k,v in os.environ.items() if not k.startswith(('JMCL_', 'CARGO_', 'RUST'))}
    env.update(CARGO_TARGET_DIR=str(owned/'target'), TMP=str(owned), TEMP=str(owned), TMPDIR=str(owned), RUSTUP_TOOLCHAIN='1.99.1', CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0')
    command = ['cargo', 'test', '--offline', '--locked', '--manifest-path', 'rust/Cargo.toml', '--target', TARGET, '--test', 'content_adopt', TEST, '--', '--exact', '--nocapture', '--test-threads=1']
    code, raw = execute(command, root, env)
    validate_result(code, raw)
    binaries = list((owned/'target'/TARGET/'debug/deps').glob('content_adopt-*.exe'))
    require(len(binaries) == 1, 'ambiguous test executable')
    data = binaries[0].read_bytes()
    validate_pe(data)
    require(not git('status', '--porcelain'), 'source mutated')
    print('VERIFIED '+json.dumps(dict(commit=git('rev-parse','HEAD'), tree=git('rev-parse','HEAD^{tree}'), production_rust_tree=SOURCE_TREE, test_sha256=TEST_SHA256, executable_sha256=hashlib.sha256(data).hexdigest(), executable_bytes=len(data), kind='PE32+/x86_64', host=rust, passed=1, failed=0, ignored=0)), flush=True)

if __name__ == '__main__':
    main()
