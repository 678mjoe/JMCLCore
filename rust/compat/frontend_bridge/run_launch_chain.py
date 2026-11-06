#!/usr/bin/env python3
"""Explicit local native Session -> Core launch.execute -> benign native child.

Linux x86_64 only. Offline builds; mandatory inherited network guard.
The original guard remains default; --launch-only-guard opts into the separately
reviewed local-recv exception, with mandatory self-tests and benign spawn gates.
Never runs the legacy archive/Java bridge tests or any actual Java executable.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

import network_guard
import launch_network_guard
from run_instance_chain import FRONTEND_REVISION, MODULES, digest

CURRENT_FRONTEND = "90b9d5472e620255af740b55315e2cf75c1c9115"
TEST = "isolated_native_session_launch_execute_chain"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frontend", required=True, type=Path)
    parser.add_argument("--current-frontend-repo", required=True, type=Path,
                        help="Local Git repository containing current frontend main; never fetches")
    parser.add_argument("--rust", required=True, type=Path)
    parser.add_argument("--evidence-dir", type=Path,
                        help="Save original Session trace and provenance; must not exist")
    parser.add_argument("--build-only", action="store_true")
    parser.add_argument("--launch-only-guard", action="store_true",
                        help="Use reviewed launch_network_guard.py and mandatory benign spawn gates")
    args = parser.parse_args()
    if args.evidence_dir and (args.build_only or args.evidence_dir.exists()):
        raise SystemExit("Evidence needs an executed test and a new output directory")
    selected_guard = launch_network_guard if args.launch_only_guard else network_guard
    selected_guard.platform_check()
    base = Path(__file__).resolve().parent
    frontend = args.frontend.resolve()
    revision = subprocess.check_output(["git", "-C", str(frontend), "rev-parse", "HEAD"], text=True).strip()
    if revision != FRONTEND_REVISION:
        raise SystemExit(f"Expected clean frontend {FRONTEND_REVISION}, got {revision}")
    subprocess.run(["git", "-C", str(frontend), "diff", "--exit-code", "HEAD", "--"], check=True)
    source_revision = subprocess.check_output(["git", "-C", str(base.parents[2]), "rev-parse", "HEAD"], text=True).strip()
    hashes = {}
    for name in MODULES:
        current = subprocess.check_output(["git", "-C", str(args.current_frontend_repo.resolve()),
                                           "show", f"{CURRENT_FRONTEND}:src-tauri/src/{name}"])
        hashes[name] = digest(frontend / "src-tauri/src" / name)
        if hashlib.sha256(current).hexdigest() != hashes[name]:
            raise RuntimeError(f"Pinned/current frontend module differs: {name}")
    print(json.dumps({"frontend_revision": revision, "current_frontend_revision": CURRENT_FRONTEND,
                      "identical_native_module_sha256": hashes}), flush=True)
    with tempfile.TemporaryDirectory(prefix="jmcl-native-launch-chain-") as temporary:
        root = Path(temporary)
        harness = root / "harness"
        shutil.copytree(base, harness, ignore=shutil.ignore_patterns("target", "__pycache__"))
        for name in MODULES:
            shutil.copy2(frontend / "src-tauri/src" / name, harness / "src" / name)
            assert digest(harness / "src" / name) == hashes[name]
        frozen = root / "core"
        shutil.copy2(args.rust.resolve(), frozen)
        core_hash = digest(frozen)
        child = root / "benign-native-child"
        subprocess.run(["rustc", "--edition=2021", "--crate-name", "jmcl_native_launch_child",
                        str(harness / "tests/fixtures/launch_child.rs"), "-o", str(child)],
                       check=True, close_fds=True, timeout=60)
        child_hash = digest(child)
        spawn_gate = root / "benign-spawn-gate"
        if args.launch_only_guard:
            subprocess.run(["rustc", "--edition=2021", "--crate-name", "jmcl_launch_spawn_gate",
                            str(harness / "tests/fixtures/launch_spawn_gate.rs"), "-o", str(spawn_gate)],
                           check=True, close_fds=True, timeout=60)
        command = ["cargo", "test", "--offline", "--locked", "--manifest-path", str(harness / "Cargo.toml"),
                   "--features", "local-native-launch-chain", "--test", "launch_chain", "--no-run", "--message-format=json"]
        built = subprocess.run(command, check=True, stdout=subprocess.PIPE, text=True, timeout=600, close_fds=True)
        artifacts = []
        for line in built.stdout.splitlines():
            row = json.loads(line)
            if row.get("reason") == "compiler-artifact" and row.get("target", {}).get("name") == "launch_chain" and row.get("executable"):
                artifacts.append(row["executable"])
        if len(artifacts) != 1:
            raise RuntimeError(f"Expected exactly one test executable, got {artifacts}")
        print(json.dumps({"core_sha256": core_hash, "native_child_sha256": child_hash,
                          "test_binary_sha256": digest(Path(artifacts[0])),
                          "guard": selected_guard.__name__,
                          "guard_sha256": digest(Path(selected_guard.__file__)),
                          "spawn_gate_sha256": digest(spawn_gate) if args.launch_only_guard else None,
                          "source_commit": source_revision}), flush=True)
        if args.build_only:
            print("Offline locked build only; guard/Core/child NOT executed.")
            return
        selected_guard.selftest()
        # Never start Core before both positive/error std spawn gates succeed.
        guard_prefix = [sys.executable, "-I", "-S", str(base / f"{selected_guard.__name__}.py"), "--exec"]
        gate_results = []
        if args.launch_only_guard:
            for mode, expected in [("missing", "missing spawn returned NotFound\n"),
                                   ("success", "benign child started\nsuccessful spawn returned exit 0\n")]:
                gate = subprocess.run(guard_prefix + [str(spawn_gate), mode], cwd=root,
                                      close_fds=True, timeout=30, text=True, capture_output=True)
                print(json.dumps({"spawn_gate": mode, "returncode": gate.returncode,
                                  "stdout": gate.stdout, "stderr": gate.stderr}), flush=True)
                gate.check_returncode()
                if gate.stdout != expected or gate.stderr:
                    raise RuntimeError(f"Unexpected benign {mode} spawn result")
                gate_results.append({"mode": mode, "returncode": gate.returncode, "stdout": gate.stdout})
        raw = root / "session-trace.jsonl"
        env = dict(os.environ, JMCL_BRIDGE_BINARY=str(frozen), JMCL_NATIVE_CHAIN_GUARDED="1",
                   JMCL_NATIVE_LAUNCH_CHILD=str(child), JMCL_NATIVE_LAUNCH_TRACE=str(raw),
                   JMCL_NATIVE_LAUNCH_CANARY="must-not-reach-launched-child",
                   JAVA_TOOL_OPTIONS="must-not-reach-probe-or-child",
                   _JAVA_OPTIONS="must-not-reach-probe-or-child",
                   JDK_JAVA_OPTIONS="must-not-reach-probe-or-child")
        guarded = guard_prefix + [artifacts[0]]
        listing = subprocess.check_output(guarded + ["--list"], cwd=root, env=env,
                                          close_fds=True, text=True, timeout=30)
        if [line for line in listing.splitlines() if line.endswith(": test")] != [TEST + ": test"]:
            raise RuntimeError(f"Expected exactly the intended test, got {listing!r}")
        result = subprocess.run(guarded + ["--exact", TEST, "--test-threads=1", "--nocapture"],
                                cwd=root, env=env, close_fds=True, timeout=60, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        print(result.stdout, flush=True)
        result.check_returncode()
        if "test result: ok. 1 passed; 0 failed; 0 ignored;" not in result.stdout:
            raise RuntimeError("Expected exactly one passed, unignored native launch test")
        if digest(frozen) != core_hash or digest(args.rust.resolve()) != core_hash or digest(child) != child_hash:
            raise RuntimeError("Core or compiled child changed during execution")
        records = [json.loads(line) for line in raw.read_text().splitlines()]
        fixture = [row["root"] for row in records if row["type"] == "fixture"]
        if len(fixture) != 1:
            raise RuntimeError("Expected exactly one owned fixture")
        def normalize(value):
            if isinstance(value, str):
                return value.replace(fixture[0], "${FIXTURE_ROOT}")
            if isinstance(value, list):
                return [normalize(item) for item in value]
            if isinstance(value, dict):
                return {key: normalize(item) for key, item in value.items()}
            return value
        normalized = json.dumps({"schema_version": 1, "records": normalize(records)}, indent=2) + "\n"
        files = ["run_launch_chain.py", "run_instance_chain.py", "network_guard.py", "Cargo.toml", "Cargo.lock",
                 "tests/launch_chain.rs", "tests/fixtures/launch_child.rs"]
        if args.launch_only_guard:
            files += ["launch_network_guard.py", "tests/fixtures/launch_spawn_gate.rs"]
        evidence = {"schema_version": 1, "source_commit": source_revision,
                    "source_sha256": {name: digest(base / name) for name in files},
                    "frontend_revision": revision, "identical_current_frontend_revision": CURRENT_FRONTEND,
                    "frontend_module_sha256": hashes, "core_sha256": core_hash,
                    "native_child_sha256": child_hash, "test_binary_sha256": digest(Path(artifacts[0])),
                    "test": TEST, "passed": 1, "failed": 0, "ignored": 0,
                    "executing_guard": selected_guard.__name__,
                    "guard_selftests": "passed immediately before execution; original network_guard.py unchanged",
                    "spawn_gates": gate_results,
                    "raw_sha256": digest(raw), "normalized_sha256": hashlib.sha256(normalized.encode()).hexdigest(),
                    "normalization": "Recursive fixture-root literal replacement only; IDs, PID, duration, RSS, native nonce, events and ordering unchanged",
                    "capture_boundary": "Original native Session callback objects and returned Result/SessionError, not byte-for-byte Core wire capture. Contains only synthetic auth in request params.",
                    "limits": "Local Linux x86_64 only; no Tauri/UI, real account, Java/JVM/game, archive/runtime acquisition, cancellation or cross-platform acceptance."}
        print(json.dumps(evidence, indent=2), flush=True)
        if args.evidence_dir:
            output = args.evidence_dir.resolve()
            output.mkdir(parents=True, exist_ok=False)
            shutil.copyfile(raw, output / "raw.jsonl")
            (output / "normalized.json").write_text(normalized)
            (output / "provenance.json").write_text(json.dumps(evidence, indent=2) + "\n")
            (output / "test-output.txt").write_text(result.stdout)
    print("Passed real native Session/Core/native-child launch chain; NOT Java/game/UI acceptance.")


if __name__ == "__main__":
    main()
