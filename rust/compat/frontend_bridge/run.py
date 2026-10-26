#!/usr/bin/env python3
"""Pinned frontend + real binaries, synthetic local data; never changes shipping pins."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

FRONTEND_REVISION = "91ec19d8125942296054d076013a7ae538f64c5d"
MODULES = ["session.rs", "transport.rs", "modpack_preview.rs"]


def run(args, **kwargs):
    print("+", " ".join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), check=True, timeout=600, **kwargs)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frontend", required=True, type=Path)
    parser.add_argument("--rust", required=True, type=Path)
    parser.add_argument("--zig", required=True, type=Path)
    parser.add_argument("--bun", default="bun")
    args = parser.parse_args()
    frontend = args.frontend.resolve()
    revision = subprocess.check_output(["git", "-C", str(frontend), "rev-parse", "HEAD"], text=True).strip()
    if revision != FRONTEND_REVISION:
        raise SystemExit(f"Expected frontend {FRONTEND_REVISION}, got {revision}")
    run(["git", "-C", frontend, "diff", "--exit-code", "HEAD", "--"])
    base = Path(__file__).resolve().parent
    with tempfile.TemporaryDirectory(prefix="jmcl-frontend-bridge-") as temporary:
        root = Path(temporary)
        harness = root / "harness"
        shutil.copytree(base, harness, ignore=shutil.ignore_patterns("target", "__pycache__"))
        # Import unchanged frontend modules and the current real-Core integration suite.
        for name in MODULES:
            source = frontend / "src-tauri/src" / name
            shutil.copy2(source, harness / "src" / name)
            print(f"frontend module {name} sha256={digest(source)}", flush=True)
        shutil.copyfile(frontend / "src-tauri/tests/rpc_smoke.rs", harness / "tests/native.rs")
        # Isolate inherited test defaults from the checkout and freeze both subjects.
        binaries = root / "backend-binaries"
        binaries.mkdir()
        suffix = ".exe" if os.name == "nt" else ""
        subjects = []
        for label, source in [("rust", args.rust), ("zig", args.zig)]:
            source = source.resolve()
            frozen = root / (label + suffix)
            shutil.copy2(source, frozen)
            subjects.append((label, frozen))
            print(json.dumps({"frontend": revision, "subject": label, "sha256": digest(frozen)}), flush=True)
        print("=== unchanged session unit checks: MOCK transports, not sidecar acceptance ===", flush=True)
        run(["cargo", "test", "--locked", "--manifest-path", harness / "Cargo.toml",
             "--lib", "session::tests", "--", "--test-threads=1"], cwd=root)
        for label, frozen in subjects:
            print(f"=== {label}: typed wrappers (injected request adapter, NOT Tauri IPC) ===", flush=True)
            for script in ["test-account-core.ts", "test-capabilities-core.ts"]:
                run([args.bun, frontend / "scripts" / script, frozen], cwd=root)
            shutil.copy2(frozen, binaries / ("jmcl-core" + suffix))
            environment = dict(os.environ, JMCL_BRIDGE_BINARY=str(frozen))
            print(f"=== {label}: unchanged native bridge + real executable ===", flush=True)
            # The sole excluded inherited test probes system Java. This lane is
            # hermetic local synthetic data; Java detect/select has its own owner.
            run(["cargo", "test", "--locked", "--manifest-path", harness / "Cargo.toml",
                 "--test", "native", "--test", "bridge", "--", "--test-threads=1",
                 "--skip", "java_detect_returns_object", "--nocapture"], cwd=root, env=environment)
            if digest(frozen) != digest(binaries / ("jmcl-core" + suffix)):
                raise RuntimeError("Test subject changed during execution")
    print("Both frozen subjects passed bounded current-frontend synthetic bridge checks.")


if __name__ == "__main__":
    main()
