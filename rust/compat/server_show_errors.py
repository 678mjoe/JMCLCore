#!/usr/bin/env python3
"""Read-only server show/get diagnostics on owned metadata; no lifecycle calls."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

BASE = {"id": "demo", "name": "Demo", "version_id": "fake"}
def snapshot(root):
    return {str(p.relative_to(root)): (p.read_bytes() if p.is_file() else None, p.stat().st_ino, p.stat().st_mode, p.stat().st_mtime_ns, p.stat().st_nlink) for p in [root, *root.rglob("*")]}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--zig", required=True)
    parser.add_argument("--rust", required=True)
    parser.add_argument("--oracle-source", required=True)
    args = parser.parse_args()
    assert subprocess.check_output(["git", "-C", args.oracle_source, "rev-parse", "HEAD"], text=True).strip() == "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    binaries = [Path(args.zig).resolve(), Path(args.rust).resolve()]
    cases = [("defaults", json.dumps(BASE), None), ("unicode", json.dumps(dict(BASE, name="世界")), None), ("fabric", json.dumps(dict(BASE, fabric_loader="test")), None)]
    for label, changes, expected in [
        ("empty-name", {"name": ""}, "InvalidServerName"),
        ("name-null", {"name": None}, "UnexpectedToken"),
        ("name-bool", {"name": True}, "UnexpectedToken"),
        ("empty-version", {"version_id": ""}, "InvalidVersionId"),
        ("bad-source", {"source": "bad"}, "InvalidEnumTag"),
        ("schema-fraction", {"schema_version": 1.5}, "InvalidNumber"),
        ("schema-overflow", {"schema_version": 4294967296}, "Overflow"),
        ("schema-unsupported", {"schema_version": 2}, "UnsupportedServerSchema"),
        ("id-invalid", {"id": "../bad"}, "InvalidServerId"),
        ("id-mismatch", {"id": "other"}, "ServerIdMismatch"),
        ("fabric-empty", {"fabric_loader": ""}, "InvalidFabricLoaderVersion"),
        ("neo-empty", {"neoforge_version": ""}, "InvalidNeoForgeVersion"),
        ("forge-empty", {"forge_version": ""}, "InvalidForgeVersion"),
        ("loader-conflict", {"fabric_loader": "a", "forge_version": "b"}, "LoaderConflict"),
        ("java-empty", {"java_path": ""}, "InvalidJavaPath"),
        ("java-nul", {"java_path": "bad\u0000path"}, "InvalidJavaPath"),
        ("eula-type", {"accept_eula": None}, "UnexpectedToken"),
    ]:
        cases.append((label, json.dumps(dict(BASE, **changes)), expected))
    cases += [("missing-name", '{"id":"demo","version_id":"fake"}', "MissingField"), ("duplicate-name", '{"id":"demo","name":"Demo","name":"Other","version_id":"fake"}', "DuplicateField"), ("truncated", '{"id":', "UnexpectedEndOfInput"), ("wrong-shape", '[]', "UnexpectedToken"), ("missing-file", None, "FileNotFound")]
    count = 0
    with tempfile.TemporaryDirectory(prefix="owned-server-show-") as td:
        root = Path(td)
        server = root / "servers" / "demo"
        server.mkdir(parents=True)
        manifest = server / "server.json"
        sibling = root / "unrelated"
        sibling.write_bytes(b"preserve")
        os.link(sibling, root / "unrelated-hardlink")
        for label, document, expected_error in cases:
            if document is None:
                manifest.unlink(missing_ok=True)
            else:
                manifest.write_text(document)
            before = snapshot(root)
            for json_mode in [False, True]:
                results = []
                for binary in binaries:
                    command = [str(binary), "server", "show", "demo"] + (["--json"] if json_mode else [])
                    p = subprocess.run(command, capture_output=True, cwd=root, timeout=30)
                    assert snapshot(root) == before, (label, "disk changed")
                    if sys.platform != "linux":
                        assert (p.returncode, p.stdout, p.stderr) == (1, b"", b"Server management is currently supported only on Linux\n")
                    elif expected_error:
                        assert (p.returncode, p.stdout, p.stderr) == (1, b"", f"failed to load server: {expected_error}\n".encode()), (label, binary, p)
                    else:
                        assert p.returncode == 0 and not p.stderr, (label, binary, p)
                        if json_mode:
                            value = json.loads(p.stdout)["server"]
                            assert value["id"] == "demo" and value["version_id"] == "fake" and value["installed"] is False, value
                            assert value["name"] == json.loads(document)["name"]
                        else:
                            assert b"server\tdemo\n" in p.stdout and b"installed\tfalse\n" in p.stdout
                    results.append((p.returncode, p.stdout, p.stderr))
                assert results[0] == results[1], (label, results)
                count += 1
            # The CLI detail channel must never alter public RPC diagnostics.
            results = []
            wire = json.dumps({"protocol": 1, "id": "readonly", "method": "server.get", "params": {"directory": "servers", "id": "demo"}}) + "\n"
            for binary in binaries:
                p = subprocess.run([str(binary), "rpc"], input=wire.encode(), capture_output=True, cwd=root, timeout=30)
                assert p.returncode == 0 and not p.stderr
                assert snapshot(root) == before
                value = json.loads(p.stdout)
                if sys.platform == "linux" and expected_error is None:
                    assert value["result"]["id"] == "demo" and value["result"]["installed"] is False
                else:
                    code = "UNSUPPORTED_PLATFORM" if sys.platform != "linux" else "SERVER_NOT_FOUND" if expected_error in ["FileNotFound", "InvalidServerId"] else "SERVER_LOAD_FAILED"
                    assert value["error"]["code"] == code, (label, value)
                results.append(p.stdout)
            assert results[0] == results[1], (label, results)
            count += 1
    print(f"PASS: {count} server show/get comparisons; exact diagnostics; disk unchanged; " + ("Linux manifest paths" if sys.platform == "linux" else "unsupported-platform gate only"))

if __name__ == "__main__":
    main()
