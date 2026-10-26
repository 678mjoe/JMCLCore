#!/usr/bin/env python3
"""Read-only typed manifest enum precedence; no lifecycle or installation calls."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

def snap(root):
    return {str(p.relative_to(root)): (p.read_bytes() if p.is_file() else None, p.stat().st_ino, p.stat().st_mode, p.stat().st_mtime_ns, p.stat().st_nlink) for p in [root, *root.rglob("*")]}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--zig", required=True)
    parser.add_argument("--rust", required=True)
    parser.add_argument("--oracle-source", required=True)
    args = parser.parse_args()
    assert subprocess.check_output(["git", "-C", args.oracle_source, "rev-parse", "HEAD"], text=True).strip() == "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    binaries = [Path(args.zig).resolve(), Path(args.rust).resolve()]
    base = {"id": "demo", "name": "Demo", "version_id": "fake"}
    cases = [("default", json.dumps(base), None), ("official", json.dumps(dict(base, source="official")), None), ("mirror", json.dumps(dict(base, source="bmclapi")), None)]
    for label, data in [
        ("name-semantic", dict(base, name="", source="bad")),
        ("id-semantic", dict(base, id="../bad", source="bad")),
        ("version-semantic", dict(base, version_id="", source="bad")),
        ("schema-semantic", dict(base, schema_version=2, source="bad")),
        ("loader-conflict", dict(base, fabric_loader="x", forge_version="x", source="bad")),
        ("missing-name", {"id": "demo", "version_id": "fake", "source": "bad"}),
        ("unicode-tag", dict(base, source="未知")),
    ]:
        cases.append((label, json.dumps(data), "InvalidEnumTag"))
    for token in ["null", "true", "[]", "{}"]:
        cases.append(("wrong-type-" + token, '{"name":"","source":' + token + '}', "UnexpectedToken"))
    cases += [
        ("valid-tag-missing-name", '{"id":"demo","version_id":"fake","source":"official"}', "MissingField"),
        ("valid-tag-semantic-name", json.dumps(dict(base, name="", source="official")), "InvalidName"),
        ("duplicate-source", '{"id":"demo","name":"Demo","version_id":"fake","source":"official","source":"bmclapi"}', "DuplicateField"),
        ("bad-source-before-duplicate", '{"source":"bad","source":"official"}', "InvalidEnumTag"),
        ("duplicate-before-bad-source", '{"name":"A","name":"B","source":"bad"}', "DuplicateField"),
        ("bad-source-before-duplicate-name", '{"source":"bad","name":"A","name":"B"}', "InvalidEnumTag"),
        ("overflow-before-enum", '{"schema_version":4294967296,"source":"bad"}', "Overflow"),
        ("enum-before-overflow", '{"source":"bad","schema_version":4294967296}', "InvalidEnumTag"),
        ("valid-tag-truncated", '{"source":"official",', "UnexpectedEndOfInput"),
    ]
    ordinals = {}
    for index, (token, tag) in enumerate([("0", "official"), ("1", "bmclapi"), ('"0"', "official"), ('"1"', "bmclapi"), ('"+1"', "bmclapi"), ('"0_1"', "bmclapi"), ('"0__1"', "bmclapi"), ('"-0"', None), ('"-00"', "official"), ('"-0_0"', "official"), ("-0", None), ('"01"', "bmclapi"), ("2", None), ("-1", None), ("1.0", None), ("1e0", None), ('"1.0"', None), ('"1e0"', None), ('"1_"', None), ('"_1"', None)]):
        label = f"ordinal-{index}"
        cases.append((label, json.dumps(base)[:-1] + ',"source":' + token + '}', None if tag else "InvalidEnumTag"))
        if tag: ordinals[label] = tag
    count = 0
    with tempfile.TemporaryDirectory(prefix="owned-manifest-enum-") as td:
        root = Path(td)
        for kind in ["instance", "server"]:
            d = root / (kind + "s") / "demo"
            d.mkdir(parents=True)
            path = d / (kind + ".json")
            for label, document, expected in cases:
                path.write_text(document)
                before = snap(root)
                native = kind == "instance" or sys.platform == "linux"
                error = ("InvalidInstanceName" if kind == "instance" else "InvalidServerName") if expected == "InvalidName" else expected
                for as_json in [False, True]:
                    values = []
                    for binary in binaries:
                        p = subprocess.run([str(binary), kind, "show", "demo"] + (["--json"] if as_json else []), cwd=root, capture_output=True, timeout=30)
                        assert snap(root) == before, (kind, label, "disk changed")
                        if not native:
                            assert (p.returncode, p.stdout, p.stderr) == (1, b"", b"Server management is currently supported only on Linux\n")
                        elif error:
                            assert (p.returncode, p.stdout, p.stderr) == (1, b"", f"failed to load {kind}: {error}\n".encode()), (kind, label, binary, p)
                        else:
                            assert p.returncode == 0 and not p.stderr, (kind, label, p)
                            if as_json:
                                value = json.loads(p.stdout)[kind]
                                assert value["id"] == "demo" and value["name"] == "Demo"
                                assert value["source"] == ordinals.get(label, "bmclapi" if label == "mirror" else "official")
                        values.append((p.returncode, p.stdout, p.stderr))
                    assert values[0] == values[1], (kind, label, values)
                    count += 1
                for method in ["get", "list"]:
                    wire = json.dumps({"protocol": 1, "id": "enum", "method": kind + "." + method, "params": {"directory": kind + "s", "id": "demo"}}) + "\n"
                    values = []
                    for binary in binaries:
                        p = subprocess.run([str(binary), "rpc"], input=wire.encode(), cwd=root, capture_output=True, timeout=30)
                        assert p.returncode == 0 and not p.stderr and snap(root) == before
                        v = json.loads(p.stdout)
                        if not native: assert v["error"]["code"] == "UNSUPPORTED_PLATFORM"
                        elif method == "list": assert len(v["result"][kind + "s"]) == (0 if error else 1)
                        elif error: assert v["error"]["code"] == kind.upper() + "_LOAD_FAILED"
                        else: assert v["result"]["id"] == "demo" and v["result"]["name"] == "Demo"
                        values.append(p.stdout)
                    assert values[0] == values[1], (kind, label, values)
                    count += 1
    print(f"PASS: {count} typed manifest source-enum comparisons; read-only; disk unchanged; non-Linux server gate preserved")

if __name__ == "__main__":
    main()
