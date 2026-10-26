#!/usr/bin/env python3
"""Bounded synthetic public metadata only; no authentication or external data."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

ACCOUNT = {"schema_version": 1, "id": "0123456789abcdef0123456789abcdef", "player_name": "Player_1", "client_id": "00001111-aaaa-2222-bbbb-3333cccc4444", "xuid": "123456"}
SIBLING = dict(ACCOUNT, id="11111111111111111111111111111111", player_name="Sibling")

def encode(value):
    return json.dumps(value, separators=(",", ":"))

def snapshot(root, identity=True):
    result = {}
    for p in [root, *root.rglob("*")]:
        st = p.stat()
        state = (p.read_bytes() if p.is_file() else None,)
        if identity:
            state += (st.st_ino, st.st_mode, st.st_mtime_ns, st.st_nlink)
        result[str(p.relative_to(root))] = state
    return result

def setup(root):
    root.mkdir()
    store = root / "accounts"
    store.mkdir()
    path = store / (ACCOUNT["id"] + ".json")
    path.write_text(encode(ACCOUNT))
    (store / (SIBLING["id"] + ".json")).write_text(encode(SIBLING))
    os.link(path, root / "untouched-hardlink")
    (root / "blocked").write_text("not a directory")

def request(binary, root, method, params):
    wire = '{"protocol":1,"id":"bytes","method":"account.microsoft.' + method + '","params":' + params + '}\n'
    p = subprocess.run([str(binary), "rpc"], input=wire.encode(), capture_output=True, cwd=root, timeout=30)
    assert p.returncode == 0 and not p.stderr, (method, p)
    return p.stdout, json.loads(p.stdout)

def error(value, message, code="INVALID_PARAMS"):
    assert value.get("error") == {"code": code, "message": message}, value

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--zig", required=True)
    parser.add_argument("--rust", required=True)
    parser.add_argument("--oracle-source", required=True)
    args = parser.parse_args()
    assert subprocess.check_output(["git", "-C", args.oracle_source, "rev-parse", "HEAD"], text=True).strip() == "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    binaries = [Path(args.zig).resolve(), Path(args.rust).resolve()]
    count = 0
    with tempfile.TemporaryDirectory(prefix="synthetic-account-params-") as td:
        parent = Path(td)
        # Exact parse-vs-semantic precedence with reads; both roots already exist.
        cases = []
        for field, method, text in [("id", "get", ACCOUNT["id"]), ("directory", "list", "accounts")]:
            head, tail = str(ord(text[0])), list(text[1:].encode())
            for label, token, category in [
                ("integer", head, "success"), ("quoted", encode(head), "success"),
                ("float", head + ".0", "success"), ("rounded-f64", head + ".0000000000000001", "success"),
                ("quoted-fraction", encode(head + ".0000000000000001"), "parse"),
                ("nonutf8", "255", "metadata"), ("nul", "0", "metadata"),
                ("overflow", "256", "parse"), ("negative", "-1", "parse"),
                ("null", "null", "parse"), ("bool", "true", "parse"),
                ("object", "{}", "parse"), ("nested", "[]", "parse"),
            ]:
                params = '{"' + field + '":[' + token + ',' + ','.join(map(str, tail)) + ']}'
                cases.append((field + "-" + label, method, params, category))
            for label, token, category in [("empty", "[]", "empty"), ("null-field", "null", "parse"), ("unicode", '"ÿ"', "metadata")]:
                cases.append((field + "-" + label, method, '{"' + field + '":' + token + '}', category))
        cases += [
            ("empty-root-invalid-id", "get", '{"directory":[],"id":[255]}', "empty"),
            ("empty-id-invalid-root", "get", '{"directory":[255],"id":[]}', "empty"),
            ("empty-root-malformed-id", "get", '{"directory":[],"id":[256]}', "parse"),
            ("nonutf8-root", "get", encode({"directory": [255], "id": ACCOUNT["id"]}), "metadata"),
            ("blocked-root-invalid-id", "get", '{"directory":"blocked","id":[255]}', "store"),
            ("blocked-root-malformed-id", "get", '{"directory":"blocked","id":[256]}', "parse"),
        ]
        cases += [(label + "-delete", "delete", params, category) for label, method, params, category in cases if method == "get" and category != "success"]
        for label, method, params, category in cases:
            outputs = []
            for side, binary in enumerate(binaries):
                root = parent / f"read-{count}-{side}"
                setup(root)
                before = snapshot(root)
                out, value = request(binary, root, method, params)
                assert snapshot(root) == before, (label, side, "read mutated disk")
                if category == "success":
                    expected = {"stage": "account_get", "account": ACCOUNT} if method == "get" else {"stage": "account_list", "accounts": [ACCOUNT, SIBLING]}
                    assert value.get("result") == expected, (label, side, value)
                elif category == "parse":
                    error(value, "Account target parameters are invalid" if method in ["get", "delete"] else "Account list parameters are invalid")
                elif category == "empty":
                    error(value, "Account directory and ID must not be empty" if method in ["get", "delete"] else "Account directory must not be empty")
                elif category == "metadata":
                    error(value, "Account metadata is invalid")
                else:
                    error(value, "Account registry operation failed", "ACCOUNT_STORE_ERROR")
                outputs.append(out)
            assert outputs[0] == outputs[1], (label, outputs)
            count += 1
        # Save parsing uses the same dynamic u8 grammar, not disk typed f128.
        for field in ["id", "player_name", "client_id", "xuid"]:
            text = ACCOUNT[field]
            head, tail = str(ord(text[0])), ','.join(map(str, text[1:].encode()))
            for label, token, category in [("integer", head, "success"), ("quoted", encode(head), "success"), ("rounded-f64", head + ".0000000000000001", "success"), ("quoted-fraction", encode(head + ".0000000000000001"), "parse"), ("nonutf8", "255", "metadata"), ("nul", "0", "metadata"), ("overflow", "256", "parse"), ("negative", "-1", "parse"), ("bool", "true", "parse"), ("null", "null", "parse"), ("object", "{}", "parse"), ("nested", "[]", "parse")]:
                account = dict(ACCOUNT)
                account[field] = "TOKEN"
                params = encode({"account": account, "ignored": {"unrecognized": [1, None, {}]}}).replace('"TOKEN"', '[' + token + ',' + tail + ']')
                outputs = []
                disks = []
                for side, binary in enumerate(binaries):
                    root = parent / f"save-{count}-{side}"
                    setup(root)
                    before = snapshot(root)
                    out, value = request(binary, root, "save", params)
                    if category == "success":
                        assert value.get("result") == {"stage": "save", "account": ACCOUNT}, (field, label, side, value)
                        assert json.loads((root / "accounts" / (ACCOUNT["id"] + ".json")).read_text()) == ACCOUNT
                        after = snapshot(root)
                        assert after["untouched-hardlink"][:4] == before["untouched-hardlink"][:4]
                        assert after["untouched-hardlink"][4] == before["untouched-hardlink"][4] - 1
                        manifest = "accounts/" + ACCOUNT["id"] + ".json"
                        assert after[manifest][1] != after["untouched-hardlink"][1]
                        sibling = "accounts/" + SIBLING["id"] + ".json"
                        assert snapshot(root)[sibling] == before[sibling]
                    else:
                        error(value, "Account metadata parameters are invalid" if category == "parse" else "Account metadata is invalid")
                        assert snapshot(root) == before, (field, label, side, "invalid save mutated disk")
                    outputs.append(out)
                    disks.append(snapshot(root, False))
                assert outputs[0] == outputs[1] and disks[0] == disks[1], (field, label)
                count += 1
        # Opening the root precedes semantic account validation, but follows parsing.
        for label, directory, byte, expected in [("blocked-semantic", "blocked", 255, "store"), ("blocked-parse", "blocked", 256, "parse"), ("new-semantic", "new-root", 255, "metadata"), ("new-parse", "new-root", 256, "parse"), ("empty-semantic", [], 255, "empty"), ("empty-parse", [], 256, "parse")]:
            outputs, disks = [], []
            for side, binary in enumerate(binaries):
                root = parent / f"order-{count}-{side}"
                setup(root)
                account = dict(ACCOUNT, player_name=[byte])
                out, value = request(binary, root, "save", encode({"directory": directory, "account": account}))
                message = {"store": "Account registry operation failed", "parse": "Account metadata parameters are invalid", "metadata": "Account metadata is invalid", "empty": "Account directory must not be empty"}[expected]
                error(value, message, "ACCOUNT_STORE_ERROR" if expected == "store" else "INVALID_PARAMS")
                assert (root / "new-root").exists() == (label == "new-semantic")
                if label == "new-semantic": assert not list((root / "new-root").iterdir())
                outputs.append(out); disks.append(snapshot(root, False))
            assert outputs[0] == outputs[1] and disks[0] == disks[1], label
            count += 1
        # Fully successful array-form save/get/list/delete, then missing get.
        results = []
        for side, binary in enumerate(binaries):
            root = parent / f"flow-{side}"
            root.mkdir()
            array_account = {k: list(v.encode()) if isinstance(v, str) else v for k, v in ACCOUNT.items()}
            target = {"directory": list(b"accounts"), "id": list(ACCOUNT["id"].encode())}
            flow = [("save", {"directory": list(b"accounts"), "account": array_account}), ("get", target), ("list", {"directory": list(b"accounts")}), ("delete", target), ("get", target)]
            records = []
            for index, (method, params) in enumerate(flow):
                out, value = request(binary, root, method, encode(params))
                if index < 4: assert "result" in value, (method, side, value)
                else: error(value, "Account metadata was not found", "ACCOUNT_NOT_FOUND")
                if method == "save": assert value["result"]["account"] == ACCOUNT
                if method == "delete":
                    assert value["result"] == {"stage": "account_delete", "id": ACCOUNT["id"], "deleted": True}
                    assert not (root / "accounts" / (ACCOUNT["id"] + ".json")).exists()
                records.append((out, snapshot(root, False)))
            results.append(records)
        assert results[0] == results[1]
        count += len(flow)
    print(f"PASS: {count} dynamic account byte-string RPC fixtures; owned metadata only; exact wire and disk effects")

if __name__ == "__main__":
    main()
