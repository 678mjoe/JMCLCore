#!/usr/bin/env python3
"""Guarded existing-root RPC dry-run only. No CLI, deletion, or default root."""
import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile

PINS = {
    "zig": {
        "src/install/artifact_store.zig": "2ff8e6e600e782d9cdff8d53ff60ce7dbd2eac7f05d1811ddfae15d6f5d2697e",
        "src/protocol/store.zig": "659a28f1ae70cae14a0acf4ba2079664504da6b27429104a725ff2f75889a962",
    },
    "rust": {
        "rust/src/store/unix.rs": "1a4e7eece2fc0dc96dd7365fea68298e5d8c3f9d57944fa93b1cc1b6771614e2",
        "rust/src/store/windows.rs": "9e4a287710801d3220262388143b2ab39824354438292bad13b4d342605b9aba",
        "rust/src/store/commands.rs": "caffbc947ff8b0dba36eaa32dfa0b8ab2896385fa3f40b1bee2f6f4cec6ca5fb",
    },
}


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def path_bytes(path):
    value = str(path)
    return value.encode("utf-8", "surrogatepass") if os.name == "nt" else os.fsencode(value)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_subject(kind, source, head, binary, expected_hash):
    actual = subprocess.run(["git", "-C", str(source), "rev-parse", "HEAD"],
                            capture_output=True, check=True, text=True).stdout.strip()
    require(actual == head, f"{kind} source HEAD changed")
    for relative, expected in PINS[kind].items():
        text = (source / relative).read_text(encoding="utf-8").replace("\r\n", "\n")
        require(hashlib.sha256(text.encode()).hexdigest() == expected,
                f"{kind} reviewed dry-run source changed: {relative}")
    require(digest(binary) == expected_hash, f"{kind} executable hash changed")


def snapshot(root):
    result = {}
    for path in [root, *sorted(root.rglob("*"), key=lambda p: path_bytes(p))]:
        s = path.lstat()
        require(not stat.S_ISLNK(s.st_mode), "fixture must not contain aliases")
        require(s.st_ino != 0, "file identity unavailable")
        regular = stat.S_ISREG(s.st_mode)
        require(regular or stat.S_ISDIR(s.st_mode), "unexpected fixture file type")
        # Access time is intentionally excluded: inspection itself can update it.
        result[path_bytes(path.relative_to(root)).hex()] = (
            s.st_mode, s.st_size, s.st_mtime_ns, s.st_ctime_ns,
            s.st_dev, s.st_ino, s.st_nlink,
            getattr(s, "st_file_attributes", None), digest(path) if regular else None,
        )
    return result


def make_store(root, peer):
    (root / "objects" / "sha1").mkdir(parents=True)
    for name, body, future, linked in [
        ("eligible", b"eligible", False, False),
        ("linked", b"link", False, True),
        ("future", b"young", True, False),
    ]:
        sha = hashlib.sha1(body).hexdigest()
        prefix = root / "objects" / "sha1" / sha[:2]
        prefix.mkdir(exist_ok=True)
        leaf = prefix / sha
        leaf.write_bytes(body)
        stamp = 2208988800 if future else 946684800  # 2040 / 2000, not near a boundary.
        os.utime(leaf, (stamp, stamp))
        if linked:
            os.link(leaf, peer)
        leaf.chmod(stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
        if name == "eligible":
            (prefix / "ignored-name").write_bytes(b"ignored")
    require(not (root / "locks").exists(), "fixture must start without lock paths")


def decode_root(value):
    if isinstance(value, str):
        return value.encode("utf-8")
    require(isinstance(value, list), "store_directory must be explicit string/byte array")
    output = bytearray()
    for item in value:
        # The harness generates only these simple exact spellings; do not accept
        # arbitrary numeric grammar as permission to select another root.
        if isinstance(item, str):
            require(item.isascii() and item.isdigit(), "unapproved quoted byte")
            number = int(item)
        elif type(item) is int:
            number = item
        elif type(item) is float:
            require(item.is_integer(), "unapproved fractional byte")
            number = int(item)
        else:
            raise AssertionError("unapproved byte type")
        require(0 <= number <= 255, "unapproved byte range")
        output.append(number)
    return bytes(output)


def guard_request(line, workspace, root):
    request = json.loads(line)
    require(set(request) == {"protocol", "id", "method", "params"}, "unexpected request shape")
    require(request["protocol"] == 1 and request["id"] == "owned-dry-run", "unexpected envelope")
    require(request["method"] == "store.gc", "only store.gc allowed")
    params = request["params"]
    require(isinstance(params, dict) and set(params) == {"store_directory", "dry_run", "min_age_seconds"},
            "explicit complete params required")
    require(params["dry_run"] is True, "dry_run must be literal true")
    require(type(params["min_age_seconds"]) is int and params["min_age_seconds"] in (0, 86400),
            "unapproved age")
    require(root.is_absolute() and root.name != ".jmclcore", "default/relative root prohibited")
    require(root.parent == workspace and root.resolve(strict=True).parent == workspace.resolve(strict=True),
            "root must be direct owned non-aliased child")
    require(stat.S_ISDIR(root.lstat().st_mode) and (root / "objects" / "sha1").is_dir(),
            "complete existing owned root required")
    expected = path_bytes(root)
    require(expected and b"\0" not in expected and decode_root(params["store_directory"]) == expected,
            "request directory differs from owned root")
    return request


def line_for(root, representation, age):
    b = path_bytes(root)
    if representation == "string":
        value = b.decode("utf-8")
    elif representation == "integers":
        value = list(b)
    elif representation == "quoted":
        value = [str(n) for n in b]
    elif representation == "floats":
        value = [float(n) for n in b]
    else:
        raise AssertionError("unapproved representation")
    return json.dumps({"protocol": 1, "id": "owned-dry-run", "method": "store.gc", "params": {
        "store_directory": value, "dry_run": True, "min_age_seconds": age,
    }}, ensure_ascii=True) + "\n"


def guard_controls(workspace, root):
    # These are guard-only negatives: never forwarded to a Core subprocess.
    good = json.loads(line_for(root, "integers", 0))
    variants = []
    for value in (False, None, 1, "true"):
        bad = json.loads(json.dumps(good)); bad["params"]["dry_run"] = value
        variants.append(bad)
    for key in ("dry_run", "store_directory"):
        bad = json.loads(json.dumps(good)); del bad["params"][key]
        variants.append(bad)
    for value in (None, "", [], ".jmclcore", str(workspace / ".jmclcore")):
        bad = json.loads(json.dumps(good)); bad["params"]["store_directory"] = value
        variants.append(bad)
    bad = json.loads(json.dumps(good)); bad["method"] = "store.other"; variants.append(bad)
    bad = json.loads(json.dumps(good)); bad["params"] = None; variants.append(bad)
    for bad in variants:
        try:
            guard_request(json.dumps(bad), workspace, root)
        except AssertionError:
            continue
        raise AssertionError("unsafe request escaped local dry-run guard")


def run_subject(binary, expected_hash, line, workspace, root):
    # These checks precede EVERY Core subprocess, not merely fixture setup.
    request = guard_request(line, workspace, root)
    require(digest(binary) == expected_hash, "executable changed before invocation")
    before = snapshot(workspace)
    try:
        completed = subprocess.run([str(binary), "rpc"], input=line.encode(),
                                   cwd=workspace, capture_output=True, timeout=30)
    finally:
        require(snapshot(workspace) == before, "dry-run changed owned tree/peer/canary")
    require(completed.returncode == 0 and completed.stderr == b"", "RPC process failed")
    lines = completed.stdout.splitlines()
    require(len(lines) == 1, "expected one terminal RPC result")
    expected = {"protocol": 1, "id": "owned-dry-run", "event": "result", "result": {
        "dry_run": True, "min_age_seconds": request["params"]["min_age_seconds"], "summary": {
            "objects_scanned": 3, "objects_eligible": 1, "objects_deleted": 0,
            "bytes_eligible": 8, "bytes_deleted": 0, "objects_linked": 1,
            "objects_young": 1, "objects_ignored": 1, "objects_busy": 0, "errors": 0,
        },
    }}
    require(json.loads(lines[0]) == expected, f"unexpected dry-run result: {completed.stdout!r}")
    return completed.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for kind in ("zig", "rust"):
        parser.add_argument(f"--{kind}", required=True, type=Path)
        parser.add_argument(f"--{kind}-sha256", required=True)
        parser.add_argument(f"--{kind}-source", required=True, type=Path)
        parser.add_argument(f"--{kind}-head", required=True)
    args = parser.parse_args()
    subjects = []
    for kind in ("zig", "rust"):
        binary = getattr(args, kind).resolve(strict=True)
        sha = getattr(args, f"{kind}_sha256")
        source = getattr(args, f"{kind}_source").resolve(strict=True)
        head = getattr(args, f"{kind}_head")
        if kind == "zig":
            require(head == "5a4310debbcee26b3d2391bab2339f79cd6ec52e", "wrong original source pin")
        verify_subject(kind, source, head, binary, sha)
        subjects.append((binary, sha))
    count = 0
    # Keep owned readonly/hardlink fixtures on all outcomes; never chmod cleanup.
    tmp = tempfile.mkdtemp(prefix="jmcl-owned-dry-run-")
    print(f"Retained owned dry-run fixture: {tmp}", flush=True)
    workspace = Path(tmp).resolve(strict=True)
    (workspace / ".jmclcore").mkdir()
    (workspace / ".jmclcore" / "default-must-not-be-used").write_bytes(b"canary")
    names = [("ascii", "root_ascii"), ("unicode", "root_目录")]
    names.append(("wtf8", "root_\ud800" if os.name == "nt" else os.fsdecode(b"root_\xed\xa0\x80")))
    if os.name != "nt":
        names.append(("ff", os.fsdecode(b"root_\xff")))
    else:
        print("EXCLUDED physical FF root: Windows native encoding cannot represent byte FF")
    for label, name in names:
        root = workspace / name
        try:
            root.mkdir()
        except OSError as error:
            if error.errno == errno.EILSEQ and label in ("wtf8", "ff"):
                require(not os.path.lexists(root), "failed raw root unexpectedly exists")
                print(f"EXCLUDED physical {label} root: filesystem EILSEQ")
                continue
            raise
        make_store(root, workspace / f"peer_{label}")
        guard_controls(workspace, root)
        representations = ["integers", "quoted", "floats"]
        if label in ("ascii", "unicode"):
            representations.insert(0, "string")
        for representation in representations:
            for age in (0, 86400):
                line = line_for(root, representation, age)
                outputs = [run_subject(binary, sha, line, workspace, root) for binary, sha in subjects]
                require(outputs[0] == outputs[1], "raw RPC result bytes differ")
                count += 1
    print(f"PASS: {count} paired explicit-root dry-run RPC outcomes; no locks/deletions/disk changes; atime excluded")


if __name__ == "__main__":
    main()
