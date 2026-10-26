#!/usr/bin/env python3
"""Read-only display-name bytes. Synthetic records, never lifecycle/Java calls."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def snapshot(root):
    result = {}
    for p in [root, *root.rglob("*")]:
        s = p.lstat()
        value = os.readlink(p) if p.is_symlink() else p.read_bytes() if p.is_file() else None
        result[str(p.relative_to(root))] = (value, s.st_ino, s.st_mode, s.st_mtime_ns, s.st_nlink)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--zig", required=True)
    parser.add_argument("--rust", required=True)
    parser.add_argument("--oracle-source", required=True)
    args = parser.parse_args()
    assert subprocess.check_output(["git", "-C", args.oracle_source, "rev-parse", "HEAD"], text=True).strip() == "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    binaries = [Path(args.zig).resolve(), Path(args.rust).resolve()]
    # Each tuple fixes expected bytes or error independently of either executable.
    cases = []
    for label, token, expected in [
        ("ascii-string", '"Demo"', b"Demo"), ("ascii-array", '[68,101,109,111]', b"Demo"),
        ("unicode-string", json.dumps("世界"), "世界".encode()),
        ("unicode-array", json.dumps(list("世界".encode())), "世界".encode()),
        ("opaque", '[255,65]', b"\xffA"),
        ("opaque-controls", '[255,9,10,0,65]', b"\xff\t\n\0A"),
        ("unicode-controls", '[65,9,10,0]', b"A\t\n\0"),
        ("nul", '[0]', b"\0"),
        ("unicode-byte-length", json.dumps(list(("界"*86).encode())), "InvalidName"), ("max-length", json.dumps([65]*256), b"A"*256),
        ("typed-grammar", '["+6__5",66.0,6.7e1]', b"ABC"),
        ("empty-string", '""', "InvalidName"), ("empty-array", '[]', "InvalidName"),
        ("oversize", json.dumps([65]*257), "InvalidName"),
        ("fraction", '[65.5]', "InvalidNumber"),
        ("precision", '[1.0000000000000001]', "InvalidNumber"),
        ("quoted-precision", '["1.0000000000000001"]', "InvalidNumber"),
        ("overflow", '[256]', "Overflow"), ("negative", '[-1]', "Overflow"),
        ("null-element", '[null]', "UnexpectedToken"), ("nested", '[[65]]', "UnexpectedToken"),
        ("bool-element", '[true]', "UnexpectedToken"), ("null", 'null', "UnexpectedToken"),
        ("number", '65', "UnexpectedToken"), ("object", '{}', "UnexpectedToken"),
    ]:
        cases.append((label, '{"id":"demo","name":'+token+',"version_id":"fake"}', expected))
    cases += [
        ("id-before-empty", '{"id":"../bad","name":[],"version_id":""}', "InvalidId"),
        ("schema-before-empty", '{"schema_version":2,"id":"demo","name":[],"version_id":""}', "UnsupportedSchema"),
        ("empty-before-version", '{"id":"demo","name":[],"version_id":""}', "InvalidName"),
        ("source-before-semantic", '{"id":"demo","name":[],"version_id":"","source":"bad"}', "InvalidEnumTag"),
        ("source-before-element", '{"source":"bad","name":[256]}', "InvalidEnumTag"),
        ("element-before-source", '{"name":[256],"source":"bad"}', "Overflow"),
        ("duplicate", '{"id":"demo","name":[65],"name":[66],"version_id":"fake"}', "DuplicateField"),
        ("unknown-huge", '{"id":"demo","name":[65],"version_id":"fake","extra":1e9999}', b"A"),
    ]
    count = 0
    with tempfile.TemporaryDirectory(prefix="owned-manifest-display-") as td:
        root = Path(td)
        sibling = root / "untouched"
        sibling.write_bytes(b"private sibling")
        os.link(sibling, root / "untouched-link")
        for kind in ["instance", "server"]:
            directory = root / (kind+"s")
            d = directory / "demo"
            d.mkdir(parents=True)
            path = d / (kind+".json")
            other = directory / "aaa"
            other.mkdir()
            (other / (kind+".json")).write_text(json.dumps({"id":"aaa","name":"Other","version_id":"fake"}))
            native = kind == "instance" or sys.platform == "linux"

            def check(label, expected, installed=False):
                nonlocal count
                before = snapshot(root)
                if isinstance(expected, bytes):
                    try: expected_json = expected.decode("utf-8")
                    except UnicodeDecodeError: expected_json = list(expected)
                    error = None
                else:
                    error = {"InvalidName":f"Invalid{kind.title()}Name", "InvalidId":f"Invalid{kind.title()}Id", "UnsupportedSchema":f"Unsupported{kind.title()}Schema"}.get(expected, expected)
                for operation in ["show", "show-json", "list", "list-json", "rpc-get", "rpc-list"]:
                    values = []
                    for binary in binaries:
                        if operation.startswith("rpc-"):
                            method = operation[4:]
                            wire = json.dumps({"protocol":1,"id":"display","method":kind+"."+method,"params":{"directory":kind+"s","id":"demo"}})+"\n"
                            p = subprocess.run([str(binary),"rpc"], input=wire.encode(), cwd=root, capture_output=True, timeout=30)
                            assert p.returncode == 0 and not p.stderr, (kind,label,operation,p)
                            value = json.loads(p.stdout)
                            if not native: assert value["error"]["code"] == "UNSUPPORTED_PLATFORM"
                            elif error and method == "get": assert value["error"]["code"] == kind.upper()+("_NOT_FOUND" if expected == "InvalidId" else "_LOAD_FAILED"), (kind,label,value)
                            else:
                                value = value["result"]
                                if method == "list":
                                    docs = value[kind+"s"]
                                    assert [v["id"] for v in docs] == (["aaa"] if error else ["aaa","demo"])
                                    value = docs[-1] if not error else None
                                if value:
                                    assert value["name"] == expected_json and value["installed"] == installed, (kind,label,value,installed)
                        else:
                            listing = operation.startswith("list")
                            as_json = operation.endswith("json")
                            argv = [str(binary),kind,"list" if listing else "show"] + ([] if listing else ["demo"]) + (["--json"] if as_json else [])
                            p = subprocess.run(argv, cwd=root, capture_output=True, timeout=30)
                            if not native:
                                assert (p.returncode,p.stdout,p.stderr) == (1,b"",b"Server management is currently supported only on Linux\n")
                            elif error and not listing:
                                assert (p.returncode,p.stdout,p.stderr) == (1,b"",f"failed to load {kind}: {error}\n".encode()), (kind,label,operation,binary,p)
                            else:
                                assert p.returncode == 0 and not p.stderr, (kind,label,operation,p)
                                if as_json:
                                    value = json.loads(p.stdout)
                                    if listing:
                                        docs = value[kind+"s"]
                                        assert [v["id"] for v in docs] == (["aaa"] if error else ["aaa","demo"])
                                        value = docs[-1] if not error else None
                                    else: value = value[kind]
                                    if value: assert value["name"] == expected_json
                                elif not error:
                                    assert (b"\t"+expected+b"\n" if listing else b"\nname\t"+expected+b"\nversion\t") in p.stdout, (kind,label,operation,p.stdout)
                        assert snapshot(root) == before, (kind,label,operation,binary,"disk changed")
                        values.append((p.returncode,p.stdout,p.stderr))
                    assert values[0] == values[1], (kind,label,operation,values)
                    count += 1

            for label, document, expected in cases:
                path.write_text(document)
                check(label, expected)
            # Display bytes do not participate in installation identity. Read only
            # prepared marker/files; never invoke installation or Java discovery.
            path.write_text('{"id":"demo","name":[255,65],"version_id":"fake"}')
            meta = d / (".minecraft/.jmcl" if kind == "instance" else ".jmcl")
            meta.mkdir(parents=True)
            marker = {"version_id":"fake","source":"official"}
            if kind == "instance": marker.update(os="linux",arch="x86_64")
            else:
                (d / "server.jar").write_bytes(b"jar")
                os.link(d / "server.jar", root / "jar-sibling")
                marker.update(schema_version=3,server_sha1="a"*40,server_size=3,java_major_version=17,launch_mode="jar",launch_file="server.jar",launch_sha1="a"*40,launch_size=3)
            for label, changes, installed in [("installed",{},True),("source-identity",{"source":"bmclapi"},kind=="instance"),("version-identity",{"version_id":"other"},False),("loader-identity",{"fabric_loader":"x"},False)]:
                (meta / "install.json").write_text(json.dumps(dict(marker,**changes)))
                check(label,b"\xffA",installed)
            (meta / "install.json").unlink()
            # Preserve the existing explicitly supported leaf read aliases. Unix
            # symlink test is visible; Windows symlink privileges not assumed.
            if os.name != "nt":
                external = root / (kind+"-aliased-document")
                path.rename(external)
                path.symlink_to(external)
                check("leaf-read-alias",b"\xffA")
                path.unlink()
                path.symlink_to(root / "missing-owned-target")
                # Missing manifest is skipped by list, show has FileNotFound,
                # RPC get has NOT_FOUND rather than LOAD_FAILED: tested separately.
                before = snapshot(root)
                for binary in binaries:
                    p = subprocess.run([str(binary),kind,"show","demo"],cwd=root,capture_output=True,timeout=30)
                    if native: assert (p.returncode,p.stdout,p.stderr)==(1,b"",f"failed to load {kind}: FileNotFound\n".encode())
                    else: assert p.returncode==1 and b"supported only on Linux" in p.stderr
                    assert snapshot(root)==before
                count += 1
                path.unlink()
            else:
                print(f"NOTE: {kind} symlink fixture not exercised on Windows; no privilege assumption")
    print(f"PASS: {count} exact manifest display-name comparisons; raw/control bytes, installation identity, ordered errors and complete disk preservation")

if __name__ == "__main__":
    main()
