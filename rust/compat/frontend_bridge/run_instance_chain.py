#!/usr/bin/env python3
"""Local opt-in real native Session + Core chain; Linux x86_64, network forbidden."""
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

# Current locally verified checkout. Independent of the legacy bridge/CI pin.
FRONTEND_REVISION = "c0107f1803c73398a99f8ddd27ed712c0ed2e1b4"
MODULES = ("session.rs", "transport.rs", "modpack_preview.rs")

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frontend", required=True, type=Path)
    parser.add_argument("--rust", required=True, type=Path)
    parser.add_argument("--scenario", choices=("instance", "content"), default="instance")
    parser.add_argument("--build-only", action="store_true", help="Compile offline; do not run guards or Core")
    args = parser.parse_args()
    target, feature, test_name = {
        "instance": ("instance_chain", "local-native-instance-chain", "isolated_native_session_cached_instance_chain"),
        "content": ("content_lifecycle", "local-native-content-lifecycle", "isolated_native_session_content_lifecycle"),
    }[args.scenario]
    network_guard.platform_check()
    frontend=args.frontend.resolve()
    revision=subprocess.check_output(["git","-C",str(frontend),"rev-parse","HEAD"],text=True).strip()
    if revision != FRONTEND_REVISION:
        raise SystemExit(f"Expected frontend {FRONTEND_REVISION}, got {revision}")
    subprocess.run(["git","-C",str(frontend),"diff","--exit-code","HEAD","--"],check=True)
    base=Path(__file__).resolve().parent
    with tempfile.TemporaryDirectory(prefix="jmcl-native-instance-chain-") as temporary:
        root=Path(temporary)
        harness=root/"harness"
        shutil.copytree(base,harness,ignore=shutil.ignore_patterns("target","__pycache__"))
        for name in MODULES:
            source=frontend/"src-tauri/src"/name
            shutil.copy2(source,harness/"src"/name)
            assert digest(source) == digest(harness/"src"/name)
            print(json.dumps({"frontend":revision,"module":name,"sha256":digest(source)}),flush=True)
        frozen=root/"core"
        shutil.copy2(args.rust.resolve(),frozen)
        frozen_digest=digest(frozen)
        print(json.dumps({"core_sha256":frozen_digest}),flush=True)
        # Build preparation only. Never execute existing archive/Java bridge tests.
        command=["cargo","test","--offline","--locked","--manifest-path",str(harness/"Cargo.toml"),
            "--features",feature,"--test",target,"--no-run","--message-format=json"]
        result=subprocess.run(command,check=True,stdout=subprocess.PIPE,text=True,timeout=600,close_fds=True)
        artifacts=[]
        for line in result.stdout.splitlines():
            row=json.loads(line)
            if row.get("reason")=="compiler-artifact" and row.get("target",{}).get("name")==target and row.get("executable"):
                artifacts.append(row["executable"])
        if len(artifacts)!=1:
            raise RuntimeError(f"Expected one owned test executable, got {artifacts}")
        if args.build_only:
            print("Offline locked build only; isolation/Core tests NOT run.",flush=True)
            return
        # Every denied syscall is actively probed before any Core is started.
        network_guard.selftest()
        env=dict(os.environ,JMCL_BRIDGE_BINARY=str(frozen),JMCL_NATIVE_CHAIN_GUARDED="1")
        guarded=[sys.executable,str(base/"network_guard.py"),"--exec",artifacts[0]]
        listing=subprocess.check_output(guarded+["--list"],cwd=root,env=env,close_fds=True,text=True,timeout=30)
        if [line for line in listing.splitlines() if line.endswith(": test")] != [test_name+": test"]:
            raise RuntimeError(f"Expected exactly the intended test; got {listing!r}")
        result=subprocess.run(guarded+["--exact",test_name,"--test-threads=1","--nocapture"],
            cwd=root,env=env,close_fds=True,timeout=60,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        print(result.stdout,flush=True)
        result.check_returncode()
        if "test result: ok. 1 passed; 0 failed; 0 ignored;" not in result.stdout:
            raise RuntimeError("Expected exactly one passed, unignored native chain test")
        if digest(frozen)!=frozen_digest or digest(args.rust.resolve())!=frozen_digest:
            raise RuntimeError("Core subject changed during test")
    print("Passed isolated real native Session/production RPC chain; NOT Tauri/UI or real game acceptance.")

if __name__ == "__main__":
    main()
