#!/usr/bin/env python3
"""Offline public-account differential and shared-disk takeover tests.

Uses only synthetic public metadata in temporary directories. Both binaries are
required: no missing-oracle skip and no live Microsoft endpoints or credentials.
"""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

ACCOUNT = {"id": "0123456789abcdef0123456789abcdef", "player_name": "Player_1", "client_id": "00001111-aaaa-2222-bbbb-3333cccc4444"}


def request(method, params=...):
    value = {"protocol": 1, "id": "account-check", "method": "account.microsoft." + method}
    if params is not ...:
        value["params"] = params
    return value


def run(binary, cwd, requests, raw=False):
    completed = subprocess.run([binary, "rpc"], input="".join(json.dumps(r) + "\n" for r in requests), text=True, capture_output=True, cwd=cwd, timeout=30)
    if completed.returncode != 0:
        raise AssertionError(f"{binary} exited {completed.returncode}: {completed.stderr}")
    result = [json.loads(line) for line in completed.stdout.splitlines()]
    assert len(result) == len(requests), (result, requests)
    return completed.stdout if raw else result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--zig", required=True)
    parser.add_argument("--rust", required=True)
    args = parser.parse_args()
    zig, rust = str(Path(args.zig).resolve()), str(Path(args.rust).resolve())
    cases = [request("list"), request("list", None), request("list", {}), request("list", []), request("list", ["accounts"]), request("save", {"account": [1, ACCOUNT["id"], "Player", ACCOUNT["client_id"], None]}), request("list", {"directory": None}), request("list", {"directory": ""}), request("list", {"directory": "../escape"}), request("get"), request("get", {}), request("get", {"id": ""}), request("get", {"id": "bad"}), request("get", {"id": ACCOUNT["id"]}), request("save"), request("save", {}), request("save", {"account": dict(ACCOUNT, access_token="synthetic-must-not-persist"), "ignored": "not-persisted"}), request("get", {"id": ACCOUNT["id"]}), request("list")]
    for version in [1, 1.0, "1", "+1", "01", "1.0", "1e0", " 1", "0x1", 2, None, True]:
        cases.append(request("save", {"account": dict(ACCOUNT, schema_version=version)}))
    for field, value in [("id", "A" * 32), ("player_name", "bad name"), ("client_id", "not-a-client-id"), ("xuid", "abc"), ("xuid", ""), ("xuid", None), ("xuid", "123")]:
        cases.append(request("save", {"account": dict(ACCOUNT, **{field: value})}))
    cases += [request("delete", {"id": ACCOUNT["id"]}), request("delete", {"id": ACCOUNT["id"]}), request("list")]
    with tempfile.TemporaryDirectory(prefix="jmcl-accounts-parity-") as directory:
        root = Path(directory)
        (root / "zig").mkdir(); (root / "rust").mkdir()
        expected, actual = run(zig, root / "zig", cases), run(rust, root / "rust", cases)
        for index, (left, right) in enumerate(zip(expected, actual)):
            assert left == right, f"case {index}: {cases[index]}\nZig={left}\nRust={right}"
        assert run(zig, root / "zig", cases, raw=True) == run(rust, root / "rust", cases, raw=True), "account wire bytes differ"
        # Two-way takeover of the exact same directory, without conversion.
        shared = root / "shared"; shared.mkdir()
        saved = run(zig, shared, [request("save", {"account": ACCOUNT})])[0]
        got = run(rust, shared, [request("get", {"id": ACCOUNT["id"]})])[0]
        assert saved["result"]["account"] == got["result"]["account"]
        updated = dict(ACCOUNT, player_name="RustUpdated", xuid="456")
        saved = run(rust, shared, [request("save", {"account": updated, "access_token": "synthetic-must-not-persist"})])[0]
        got = run(zig, shared, [request("get", {"id": ACCOUNT["id"]})])[0]
        assert saved["result"]["account"] == got["result"]["account"]
        disk = (shared / "accounts" / (ACCOUNT["id"] + ".json")).read_text()
        assert "synthetic-must-not-persist" not in disk and "access_token" not in disk
        assert json.loads(disk) == dict(updated, schema_version=1)
        # Corrupt, mismatched, schema-less and oversized stored manifests remain
        # undeletable through the managed API and are omitted from listing.
        for bad in ["{}", "not-json", json.dumps(dict(ACCOUNT, schema_version=2)), json.dumps(dict(ACCOUNT, schema_version=1, id="f" * 32)), " " * 16385]:
            (shared / "accounts" / (ACCOUNT["id"] + ".json")).write_text(bad)
            checks = [request("get", {"id": ACCOUNT["id"]}), request("list"), request("delete", {"id": ACCOUNT["id"]})]
            assert run(zig, shared, checks) == run(rust, shared, checks)
            assert (shared / "accounts" / (ACCOUNT["id"] + ".json")).read_text() == bad
    print(f"PASS: {len(cases)} account RPC cases, bidirectional shared-directory takeover, and 5 corrupt-manifest cases")


if __name__ == "__main__":
    main()
