#!/usr/bin/env python3
"""Two typed wrappers + six exact native Session contracts; never Tauri/GUI acceptance."""
import argparse
from collections import Counter
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
import tempfile

CORE_COMMIT = "bdd6cb3e55d69cb9e0e02037d251635e289207a8"
CORE_TREE = "0e8e901bc587cf3c6d7dc1ba78821c4701785e20"
FRONTEND_COMMIT = "9c0fd81ce8109c1e35229560fee31308ca114ef5"
FRONTEND_TREE = "48ea91981bdb7b375bca8e9bc47738b6c3dde4d1"
RUST_VERSION = "1.99.1"
BUN_VERSION = "1.4.4"
HOSTS = {
    "ubuntu-24.04": ("Linux", "x86_64", "x86_64-unknown-linux-gnu"),
    "macos-15": ("Darwin", "arm64", "aarch64-apple-darwin"),
    "macos-15-intel": ("Darwin", "x86_64", "x86_64-apple-darwin"),
    "windows-2025": ("Windows", "AMD64", "x86_64-pc-windows-msvc"),
}
SELECTORS = (
    "handshake_reports_protocol_v1",
    "ping_error_mapping_and_session_survival",
    "java_runtime_list_on_empty_store",
    "instance_status_matches_version_and_loader_but_not_source",
    "concurrent_callers_on_one_session_are_serialized",
    "account_metadata_requires_account_envelope_and_round_trips",
)
UNRUN_INVENTORY = (
    "java_detect_returns_object",
    "worlds_lifecycle_round_trip",
    "bounded_modpack_preview_matches_real_core_import_metadata_without_network",
)
WRAPPERS = {
    "test-account-core.ts": "Typed account save/get/list/update/delete passed against real Core; synthetic metadata only.",
    "test-capabilities-core.ts": "Typed Java policy/exclusivity and download-bound guards passed against real Core; no network, Java execution, or downloads.",
}
MODULES = ("session.rs", "transport.rs", "modpack_preview.rs")
BASE = Path(__file__).resolve().parent


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def output(command, **kwargs):
    return subprocess.check_output(list(map(str, command)), text=True, **kwargs).strip()


def verify_source(path, commit, tree, allow_generated=False):
    require(path.is_dir() and not path.is_symlink(), f"Not a real source directory: {path}")
    require(output(["git", "-C", path, "rev-parse", "HEAD"]) == commit, "Wrong source commit")
    require(output(["git", "-C", path, "rev-parse", "HEAD^{tree}"]) == tree, "Wrong source tree")
    require(not output(["git", "-C", path, "status", "--porcelain=v1", "--untracked-files=no"]), "Tracked source changed")
    extra = output(["git", "-C", path, "ls-files", "--others", "--exclude-standard"]).splitlines()
    allowed = (".native-contract-harness/", "backend-binaries/jmcl-core") if allow_generated else ()
    require(all(any(p.startswith(a) for a in allowed) for p in extra), f"Unexpected untracked source files: {extra}")
    names = subprocess.check_output(["git", "-C", str(path), "ls-files", "-z"]).decode().split("\0")
    return {name: digest(path / name) for name in names if name}


def verify_host(label, rust_version, bun_version, system=None, machine=None):
    expected = HOSTS[label]
    actual = (system or platform.system(), machine or platform.machine())
    require(actual == expected[:2], f"Non-native/mismatched runner: {actual} != {expected[:2]}")
    require(re.search(r"^release: " + re.escape(RUST_VERSION) + r"$", rust_version, re.M), "Wrong Rust version")
    require(re.search(r"^host: " + re.escape(expected[2]) + r"$", rust_version, re.M), "Wrong rustc native host")
    require(bun_version.strip() == BUN_VERSION, "Wrong Bun version")
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


def verify_inventory(text, windows):
    names = re.findall(r"^(.+): test$", text, re.M)
    expected = SELECTORS + UNRUN_INVENTORY + (("local_child_has_no_console_and_keeps_rpc_pipes",) if windows else ())
    require(Counter(names) == Counter(expected), f"Inventory changed: {names}")
    return names


def verify_single(text, name):
    require(name in SELECTORS, "Selector is not allowlisted")
    require(re.findall(r"^running (\d+) tests?$", text, re.M) == ["1"], "Expected exactly one running test")
    require(re.findall(r"^test (.+) \.\.\. (\w+)$", text, re.M) == [(name, "ok")], "Wrong selected test/outcome")
    summaries = re.findall(r"^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;", text, re.M)
    require(summaries == [("ok", "1", "0", "0")], "Expected one pass, zero failures, zero ignored")


def test_command(manifest, name=None, inventory=False, build=False):
    command = ["cargo", "test", "--offline", "--locked", "--manifest-path", str(manifest), "--test", "rpc_smoke"]
    if build:
        require(name is None and not inventory, "Ambiguous command")
        return command + ["--no-run", "--message-format=json"]
    if inventory:
        require(name is None, "Ambiguous command")
        return command + ["--", "--list", "--format", "terse"]
    require(name in SELECTORS, "Selector is not allowlisted")
    return command + [name, "--", "--exact", "--nocapture", "--test-threads=1"]


def clean_environment(root, target):
    root.mkdir(parents=True, exist_ok=False)
    env = {k: v for k, v in os.environ.items() if not k.startswith("JMCL_") and k not in (
        "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER")}
    env.update({key: str(root.resolve()) for key in ("TMPDIR", "TEMP", "TMP")})
    env.update(RUSTUP_TOOLCHAIN=RUST_VERSION, CARGO_TARGET_DIR=str(target.resolve()),
               CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    return env


def prepare(frontend):
    harness = frontend / ".native-contract-harness"
    require(not harness.exists(), "Refusing to overwrite an existing harness")
    (harness / "src").mkdir(parents=True)
    (harness / "tests").mkdir()
    for name in ("Cargo.toml", "Cargo.lock"):
        shutil.copyfile(BASE / name, harness / name)
    shutil.copyfile(BASE / "lib.rs", harness / "src/lib.rs")
    for name in MODULES:
        shutil.copyfile(frontend / "src-tauri/src" / name, harness / "src" / name)
        require(digest(harness / "src" / name) == digest(frontend / "src-tauri/src" / name), "Module copy differs")
    shutil.copyfile(frontend / "src-tauri/tests/rpc_smoke.rs", harness / "tests/rpc_smoke.rs")
    require(digest(harness / "tests/rpc_smoke.rs") == digest(frontend / "src-tauri/tests/rpc_smoke.rs"), "Smoke source copy differs")
    return harness


def display_output(raw):
    # Windows hosted Python may expose a cp1252 console. Display escapes must
    # never prevent the raw log being retained or its strict validator running.
    encoding = getattr(sys.stdout, "encoding", None) or "utf-8"
    text = raw.decode("utf-8", errors="backslashreplace")
    print(text.encode(encoding, errors="backslashreplace").decode(encoding), end="", flush=True)


class Recorder:
    def __init__(self, root):
        self.root = root
        self.records = []

    def run(self, label, command, cwd, env, timeout=900):
        record = {"label": label, "command": list(map(str, command)), "cwd": str(cwd),
                  "temporary_root": env["TMPDIR"], "timeout_seconds": timeout}
        self.records.append(record)
        print("COMMAND " + json.dumps(record), flush=True)
        log = self.root / f"{len(self.records):02}-{label}.log"
        try:
            result = subprocess.run(record["command"], cwd=cwd, env=env,
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                    timeout=timeout, check=False)
        except subprocess.TimeoutExpired as exc:
            raw = exc.output or b""
            log.write_bytes(raw)
            record.update(timed_out=True, returncode=None, log_sha256=digest(log), log_bytes=len(raw))
            display_output(raw)
            raise
        raw = result.stdout
        log.write_bytes(raw)
        record.update(returncode=result.returncode, log_sha256=digest(log), log_bytes=len(raw))
        display_output(raw)
        require(result.returncode == 0, f"{label} exited {result.returncode}; see {log}")
        # Preserve raw bytes above; validation receives strict UTF-8 and explicit
        # CRLF normalization, independent of the display's replacement decoder.
        return raw.decode("utf-8").replace("\r\n", "\n")


def execute(core, frontend, label, work):
    require(not work.exists(), "Work directory must be newly owned")
    work.mkdir(parents=True)
    recorder = Recorder(work)
    report = {"status": "incomplete", "scope": "two typed wrappers and six exact native Session tests; not Tauri IPC, GUI or bundled discovery",
              "core_commit": CORE_COMMIT, "core_tree": CORE_TREE,
              "frontend_commit": FRONTEND_COMMIT, "frontend_tree": FRONTEND_TREE,
              "runner_label": label, "commands": recorder.records,
              "replacement_allowed": False, "full_compatibility": False}
    try:
        before_core = verify_source(core, CORE_COMMIT, CORE_TREE)
        before_front = verify_source(frontend, FRONTEND_COMMIT, FRONTEND_TREE, allow_generated=True)
        report["source_sha256"] = {"core": before_core, "frontend": before_front}
        rust_version = output(["rustc", "+" + RUST_VERSION, "-Vv"])
        bun_version = output(["bun", "--version"])
        target = verify_host(label, rust_version, bun_version)
        report.update(rustc=rust_version, bun=bun_version, target=target,
                      actual_host={"system": platform.system(), "machine": platform.machine(), "platform": platform.platform()})
        harness = frontend / ".native-contract-harness"
        require(harness.is_dir(), "Run --prepare first")
        for name in ("Cargo.toml", "Cargo.lock"):
            require(digest(harness / name) == digest(BASE / name), "Harness manifest/lock changed")
        require(digest(harness / "src/lib.rs") == digest(BASE / "lib.rs"), "Harness library changed")
        for name in MODULES:
            require(digest(harness / "src" / name) == digest(frontend / "src-tauri/src" / name), "Imported module changed")
        require(digest(harness / "tests/rpc_smoke.rs") == digest(frontend / "src-tauri/tests/rpc_smoke.rs"), "Imported smoke changed")
        build_env = clean_environment(work / "build-temp", work / "core-target")
        recorder.run("core-build", ["cargo", "build", "--offline", "--locked", "--manifest-path", core / "rust/Cargo.toml", "--bin", "jmcl-core-rust"], work, build_env)
        suffix = ".exe" if os.name == "nt" else ""
        built = work / "core-target/debug" / ("jmcl-core-rust" + suffix)
        report["built_core"] = verify_architecture(built, target)
        frozen = work / ("frozen-core" + suffix)
        shutil.copy2(built, frozen)
        frozen.chmod(0o555)
        stage = frontend / "backend-binaries" / ("jmcl-core" + suffix)
        require(not stage.exists(), "Refusing to overwrite an existing sidecar")
        stage.parent.mkdir(exist_ok=True)
        shutil.copy2(frozen, stage)
        report["frozen_core"] = verify_architecture(frozen, target)
        report["staged_core"] = verify_architecture(stage, target)
        expected_hash = report["built_core"]["sha256"]
        def unchanged():
            require(all(digest(p) == expected_hash for p in (built, frozen, stage)), "Core subject hash changed")
            require(verify_source(core, CORE_COMMIT, CORE_TREE) == before_core, "Core source bytes changed")
            require(verify_source(frontend, FRONTEND_COMMIT, FRONTEND_TREE, allow_generated=True) == before_front, "Frontend source bytes changed")
        unchanged()
        for script, marker in WRAPPERS.items():
            unchanged()
            env = clean_environment(work / ("tmp-" + script), work / "frontend-target")
            result = recorder.run(script, ["bun", frontend / "scripts" / script, frozen], Path(env["TMPDIR"]), env, 90)
            require(result.splitlines().count(marker) == 1, "Typed wrapper success marker missing/duplicated")
            unchanged()
        manifest = harness / "Cargo.toml"
        build_env = clean_environment(work / "frontend-build-temp", work / "frontend-target")
        build_log = recorder.run("session-build", test_command(manifest, build=True), work, build_env)
        artifacts = []
        for line in build_log.splitlines():
            if not line.startswith("{"):
                continue
            item = json.loads(line)
            if item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == "rpc_smoke" and item.get("executable"):
                artifacts.append(Path(item["executable"]).resolve())
        require(len(artifacts) == 1 and artifacts[0].is_relative_to((work / "frontend-target").resolve()), "Expected one owned native test artifact")
        report["test_artifact"] = verify_architecture(artifacts[0], target)
        inventory_env = clean_environment(work / "inventory-temp", work / "frontend-target")
        inventory = recorder.run("inventory", test_command(manifest, inventory=True), work, inventory_env)
        report["inventory"] = verify_inventory(inventory, os.name == "nt")
        report["selected"] = []
        for name in SELECTORS:
            unchanged()
            env = clean_environment(work / ("tmp-" + name), work / "frontend-target")
            result = recorder.run(name, test_command(manifest, name=name), Path(env["TMPDIR"]), env, 120)
            verify_single(result, name)
            unchanged()
            require(digest(artifacts[0]) == report["test_artifact"]["sha256"], "Test artifact changed")
            report["selected"].append({"name": name, "passed": 1, "failed": 0, "ignored": 0})
        require(verify_source(core, CORE_COMMIT, CORE_TREE) == before_core, "Core source bytes changed")
        require(verify_source(frontend, FRONTEND_COMMIT, FRONTEND_TREE, allow_generated=True) == before_front, "Frontend source bytes changed")
        unchanged()
        report.update(status="passed", typed_wrapper_count=2, native_session_count=6, source_clean_after=True,
                      immutable_subjects_after=True)
    except Exception as exc:
        report.update(status="failed_or_blocked", error=f"{type(exc).__name__}: {exc}")
        raise
    finally:
        path = work / "report.json"
        path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("FINAL_REPORT_BEGIN", flush=True)
        print(json.dumps(report, indent=2), flush=True)
        print("FINAL_REPORT_END", flush=True)
        print(f"REPORT_SHA256 {digest(path)}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core", type=Path, required=True)
    parser.add_argument("--frontend", type=Path, required=True)
    parser.add_argument("--runner", choices=HOSTS, required=True)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--prepare", action="store_true")
    args = parser.parse_args()
    core, frontend = args.core.resolve(), args.frontend.resolve()
    if args.prepare:
        verify_source(core, CORE_COMMIT, CORE_TREE)
        verify_source(frontend, FRONTEND_COMMIT, FRONTEND_TREE)
        print(prepare(frontend))
    else:
        require(args.work is not None, "--work is required for execution")
        execute(core, frontend, args.runner, args.work.resolve())


if __name__ == "__main__":
    main()
