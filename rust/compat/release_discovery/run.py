#!/usr/bin/env python3
"""Two native release-discovery scenarios; no Tauri/UI/package acceptance."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import struct
import subprocess
import sys

CORE_COMMIT = "bdd6cb3e55d69cb9e0e02037d251635e289207a8"
CORE_TREE = "0e8e901bc587cf3c6d7dc1ba78821c4701785e20"
FRONTEND_COMMIT = "9c0fd81ce8109c1e35229560fee31308ca114ef5"
FRONTEND_TREE = "48ea91981bdb7b375bca8e9bc47738b6c3dde4d1"
RUST_VERSION = "1.99.1"
HOSTS = {
    "ubuntu-24.04": ("Linux", "x86_64", "x86_64-unknown-linux-gnu"),
    "macos-15": ("Darwin", "arm64", "aarch64-apple-darwin"),
    "macos-15-intel": ("Darwin", "x86_64", "x86_64-apple-darwin"),
    "windows-2025": ("Windows", "AMD64", "x86_64-pc-windows-msvc"),
}

BASE = Path(__file__).resolve().parent

def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def output(command, **kwargs):
    return subprocess.check_output(list(map(str, command)), text=True, **kwargs).strip()


def verify_source(path, commit, tree):
    require(path.is_dir() and not path.is_symlink(), f"Not a real source directory: {path}")
    require(output(["git", "-C", path, "rev-parse", "HEAD"]) == commit, "Wrong source commit")
    require(output(["git", "-C", path, "rev-parse", "HEAD^{tree}"]) == tree, "Wrong source tree")
    require(not output(["git", "-C", path, "status", "--porcelain=v1", "--untracked-files=no"]), "Tracked source changed")
    extra = output(["git", "-C", path, "ls-files", "--others", "--exclude-standard"]).splitlines()
    require(not extra, f"Unexpected untracked source files: {extra}")
    names = subprocess.check_output(["git", "-C", str(path), "ls-files", "-z"]).decode().split("\0")
    return {name: digest(path / name) for name in names if name}


def verify_host(label, rust_version, system=None, machine=None):
    expected = HOSTS[label]
    actual = (system or platform.system(), machine or platform.machine())
    require(actual == expected[:2], f"Non-native/mismatched runner: {actual} != {expected[:2]}")
    require(re.search(r"^release: " + re.escape(RUST_VERSION) + r"$", rust_version, re.M), "Wrong Rust version")
    require(re.search(r"^host: " + re.escape(expected[2]) + r"$", rust_version, re.M), "Wrong rustc native host")
    return expected[2]


def verify_architecture(path, target):
    data = Path(path).read_bytes()
    require(len(data) >= 64, "Truncated native executable")
    if target == "x86_64-unknown-linux-gnu":
        require(data[:6] == b"\x7fELF\x02\x01", "Expected little-endian ELF64")
        require(struct.unpack_from("<H", data, 18)[0] == 62, "Wrong ELF machine")
        require(struct.unpack_from("<H", data, 16)[0] in (2, 3), "ELF is not executable/PIE")
        kind = "ELF64/x86_64"
    elif target in ("aarch64-apple-darwin", "x86_64-apple-darwin"):
        require(data[:4] == b"\xcf\xfa\xed\xfe", "Expected thin little-endian Mach-O64")
        cpu = 0x0100000C if target.startswith("aarch64") else 0x01000007
        require(struct.unpack_from("<I", data, 4)[0] == cpu, "Wrong Mach-O CPU")
        require(struct.unpack_from("<I", data, 12)[0] == 2, "Mach-O is not executable")
        kind = "Mach-O64/" + target.split("-")[0]
    elif target == "x86_64-pc-windows-msvc":
        require(data[:2] == b"MZ", "Expected real PE executable, not a renamed script")
        offset = struct.unpack_from("<I", data, 60)[0]
        require(offset + 26 <= len(data), "Truncated PE header")
        require(data[offset:offset + 4] == b"PE\0\0", "Missing PE signature")
        require(struct.unpack_from("<H", data, offset + 4)[0] == 0x8664, "Wrong PE machine")
        flags = struct.unpack_from("<H", data, offset + 22)[0]
        require(flags & 2 and not flags & 0x2000, "PE is not an executable")
        require(struct.unpack_from("<H", data, offset + 24)[0] == 0x20B, "Expected PE32+")
        kind = "PE32+/x86_64"
    else:
        raise RuntimeError("Unreviewed native target")
    return {"kind": kind, "sha256": digest(path), "bytes": len(data)}


def clean_environment(root, target):
    root.mkdir(parents=True, exist_ok=False)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("JMCL_", "CARGO_PROFILE_")) and k not in (
        "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER")}
    env.update({key: str(root.resolve()) for key in ("TMPDIR", "TEMP", "TMP")})
    env.update(RUSTUP_TOOLCHAIN=RUST_VERSION, CARGO_TARGET_DIR=str(target.resolve()),
               CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    return env


class Recorder:
    def __init__(self, root):
        self.root, self.records = root, []

    def run(self, label, command, cwd, env, timeout=900):
        record = {'label': label, 'command': list(map(str, command)), 'cwd': str(cwd),
                  'timeout_seconds': timeout}
        self.records.append(record)
        try:
            result = subprocess.run(record['command'], cwd=cwd, env=env, stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT, timeout=timeout, check=False)
            raw, code = result.stdout, result.returncode
        except subprocess.TimeoutExpired as exc:
            raw, code = exc.output or b'', None
        path = self.root / (label + '.raw')
        path.write_bytes(raw)
        record.update(returncode=code, raw_sha256=digest(path), raw_bytes=len(raw),
                      raw_base64=base64.b64encode(raw).decode('ascii'))
        # Base64 carries original subprocess pipe bytes, not decoded Actions logs.
        print('RAW_COMMAND ' + json.dumps(record), flush=True)
        require(code == 0, f'{label} failed or timed out: {code}')
        return raw


def execute(core, frontend, label, work):
    require(not work.exists(), 'Work directory must be new and exclusively owned')
    work.mkdir(parents=True)
    recorder = Recorder(work)
    report = {'status': 'incomplete', 'scope': 'two release resolver/native Session scenarios only',
              'not_covered': ['Tauri', 'UI', 'real bundle', 'installer', 'Java', 'accounts', 'network services'],
              'core_commit': CORE_COMMIT, 'core_tree': CORE_TREE,
              'frontend_commit': FRONTEND_COMMIT, 'frontend_tree': FRONTEND_TREE,
              'runner_label': label, 'commands': recorder.records, 'scenarios': [],
              'full_compatibility': False, 'replacement_allowed': False}
    try:
        source_core = verify_source(core, CORE_COMMIT, CORE_TREE)
        source_front = verify_source(frontend, FRONTEND_COMMIT, FRONTEND_TREE)
        report['source_sha256'] = {'core': source_core, 'frontend': source_front}
        rustc = output(['rustc', '+' + RUST_VERSION, '-Vv'])
        target = verify_host(label, rustc)
        report.update(rustc=rustc, target=target, actual_host={'system': platform.system(), 'machine': platform.machine()})
        for name in ('session.rs', 'transport.rs'):
            require((BASE / 'src' / name).read_bytes() == (frontend / 'src-tauri/src' / name).read_bytes(), 'Production module copy differs')
        harness_files = ('Cargo.toml', 'Cargo.lock', 'run.py', 'test_run.py', 'README.md', 'src/main.rs', 'src/session.rs', 'src/transport.rs')
        harness_hashes = {n: digest(BASE / n) for n in harness_files}
        report['harness_sha256'] = harness_hashes
        repo = BASE.parents[2]
        report['harness_commit'] = output(['git', '-C', repo, 'rev-parse', 'HEAD'])
        report['harness_tree'] = output(['git', '-C', repo, 'rev-parse', 'HEAD^{tree}'])
        report['workflow_sha256'] = digest(repo / '.github/workflows/release-core-discovery.yml')
        env = clean_environment(work / 'build-temp', work / 'target')
        # Explicit native target wins over Cargo configuration; fresh output root
        # plus native header checks prevent stale or cross-target artifacts.
        recorder.run('core-build', ['cargo', 'build', '--offline', '--locked', '--target', target,
                     '--manifest-path', core / 'rust/Cargo.toml', '--bin', 'jmcl-core-rust'], work, env)
        recorder.run('probe-build', ['cargo', 'build', '--offline', '--locked', '--release', '--target', target,
                     '--manifest-path', BASE / 'Cargo.toml'], work, env)
        suffix = '.exe' if os.name == 'nt' else ''
        built = work / 'target' / target / 'debug' / ('jmcl-core-rust' + suffix)
        probe = work / 'target' / target / 'release' / ('jmcl-release-discovery-probe' + suffix)
        report['built_core'] = verify_architecture(built, target)
        report['built_probe'] = verify_architecture(probe, target)
        core_hash = digest(built)
        frozen = work / ('frozen-core' + suffix)
        shutil.copy2(built, frozen)
        package = work.joinpath('package', *['l' + str(i) for i in range(10)])
        if platform.system() == 'Darwin':
            package = package / 'Probe.app/Contents/MacOS'
        package.mkdir(parents=True)
        executable = package / ('probe' + suffix)
        sidecar = package / ('jmcl-core' + suffix)
        shutil.copy2(probe, executable)
        shutil.copy2(frozen, sidecar)
        ancestors = list(executable.parents)[:8]
        require(len(ancestors) == 8 and all(d.is_relative_to(work) for d in ancestors), 'Search escaped owned directories')
        require(all(not (d / 'backend-binaries').exists() for d in ancestors), 'Development fallback contamination')
        require(all(d == package or not (d / sidecar.name).exists() for d in ancestors), 'Ancestor sidecar contamination')
        report['searched_ancestors'] = list(map(str, ancestors))
        report['frozen_core'] = verify_architecture(frozen, target)
        report['staged_probe'] = verify_architecture(executable, target)
        report['staged_core'] = verify_architecture(sidecar, target)
        require(digest(executable) == digest(probe), 'Probe copy changed')
        scenario_env = clean_environment(work / 'scenario-temp', work / 'target')
        cwd = work / 'unrelated-cwd'
        cwd.mkdir()
        def unchanged(core_path):
            require(all(digest(p) == core_hash for p in (built, frozen, core_path)), 'Frozen Core bytes changed')
            require(digest(executable) == report['built_probe']['sha256'], 'Probe changed')
            require({n: digest(BASE / n) for n in harness_files} == harness_hashes, 'Harness changed')
        for scenario in ('present', 'absent'):
            current_core = sidecar
            if scenario == 'absent':
                current_core = work / ('removed-sidecar' + suffix)
                sidecar.rename(current_core)
                require(not sidecar.exists(), 'Sidecar was not removed')
            unchanged(current_core)
            raw = recorder.run(scenario, [executable, scenario, target.split('-')[0]], cwd, scenario_env, 30)
            row = json.loads(raw.decode('utf-8'))
            require(row['scenario'] == scenario and row['arch'] == target.split('-')[0] and row['debug_assertions'] is False, 'Wrong scenario/native/release result')
            if scenario == 'present':
                resolved = Path(row['resolved']).resolve()
                require(resolved == sidecar.resolve() and os.path.samefile(resolved, sidecar), 'Not exact adjacent sidecar')
                require(digest(resolved) == core_hash, 'Resolved Core hash mismatch')
                require(row['identity']['protocol'] == 1 and row['identity']['name'] == 'jmcl-core', 'Wrong Core identity')
                require(row['ping'] == {'pong': True} and row['closed'] is True, 'Session sequence incomplete')
                row['resolved_sha256'] = digest(resolved)
            else:
                require(row['error'] == sidecar.name + ' not found near the application or in backend-binaries; set JMCL_CORE or pass binary_path', 'Wrong missing-Core error')
            unchanged(current_core)
            report['scenarios'].append(row)
        require([x['scenario'] for x in report['scenarios']] == ['present', 'absent'], 'Scenario count/order changed')
        require(verify_source(core, CORE_COMMIT, CORE_TREE) == source_core, 'Core source changed')
        require(verify_source(frontend, FRONTEND_COMMIT, FRONTEND_TREE) == source_front, 'Frontend source changed')
        report.update(status='passed', scenario_count=2, source_clean_after=True, immutable_subjects_after=True)
    except Exception as exc:
        report.update(status='failed_or_blocked', error=f'{type(exc).__name__}: {exc}')
        raise
    finally:
        path = work / 'report.json'
        path.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
        print('FINAL_REPORT_BEGIN', flush=True)
        print(path.read_text(encoding='utf-8'), end='', flush=True)
        print('FINAL_REPORT_END', flush=True)
        print('REPORT_SHA256 ' + digest(path), flush=True)


def main():
    if not __debug__:
        raise RuntimeError('Python optimization disables acceptance gates; run without -O/PYTHONOPTIMIZE')
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--core', type=Path, required=True)
    p.add_argument('--frontend', type=Path, required=True)
    p.add_argument('--runner', choices=HOSTS, required=True)
    p.add_argument('--work', type=Path, required=True)
    a = p.parse_args()
    execute(a.core.resolve(), a.frontend.resolve(), a.runner, a.work.resolve())


if __name__ == '__main__':
    main()
