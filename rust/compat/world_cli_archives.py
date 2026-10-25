#!/usr/bin/env python3
"""Bounded offline instance-world export/backup CLI corpus.

Requires explicit binary hashes. Copies verified bytes privately before execution;
uses only synthetic saves. Compression bytes/sizes need not match, but every
printed size must equal its own archive's actual length. No blanket normalization.
"""
import argparse
import calendar
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import zipfile

STAMP = (2020, 1, 2, 3, 4, 6)
EPOCH = calendar.timegm(STAMP)
FILES = {"level.dat": b"synthetic-not-nbt", "region/r.0.0.mca": b"private-world" * 200,
         "empty.bin": b"", "icon.png": b"synthetic-icon"}


def verified_copy(source, expected, destination):
    data = Path(source).read_bytes()
    actual = hashlib.sha256(data).hexdigest()
    assert actual == expected, (source, "binary SHA256 mismatch", actual, expected)
    destination.write_bytes(data)
    destination.chmod(0o700)
    assert hashlib.sha256(destination.read_bytes()).hexdigest() == expected
    return str(destination)


def setup(root):
    instance = root / "instances/demo"
    world = instance / ".minecraft/saves/Alpha"
    world.mkdir(parents=True)
    (instance / "instance.json").write_text(json.dumps({"schema_version": 1, "id": "demo",
        "name": "Demo", "version_id": "fixture", "source": "official"}))
    for name, data in {**FILES, "session.lock": b"unheld-lock", ".jmcl/registry": b"private",
                       ".jmcl-copy-fixture/ignored": b"temporary"}.items():
        file = world / name
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_bytes(data)
    (world / "empty-dir").mkdir()
    for file in [world, *world.rglob("*")]:
        os.utime(file, (EPOCH, EPOCH))
    return world


def snapshot(world):
    return {str(p.relative_to(world)): (p.is_dir(), p.stat().st_ino, p.stat().st_mtime_ns,
            None if p.is_dir() else p.read_bytes()) for p in [world, *world.rglob("*")]}


def run(binary, root, *args):
    p = subprocess.run([binary, "instance", "worlds", *args, "--instances-directory", "instances"],
                       cwd=root, capture_output=True, timeout=15)
    return p.returncode, p.stdout, p.stderr


def success(result, expected):
    assert result == (0, expected.encode(), b""), (result, expected)


def archive_contents(path):
    expected = {"Alpha/" + name: data for name, data in FILES.items()}
    expected.update({"Alpha/region/": b"", "Alpha/empty-dir/": b""})
    with zipfile.ZipFile(path) as archive:
        assert archive.testzip() is None
        assert len(archive.infolist()) == len(expected), archive.namelist()
        assert set(archive.namelist()) == set(expected), archive.namelist()
        for entry in archive.infolist():
            assert entry.date_time == STAMP, (entry.filename, entry.date_time)
            assert entry.file_size == len(expected[entry.filename])
            assert not entry.flag_bits & 1  # No encryption.
            assert archive.read(entry) == expected[entry.filename]
        return expected


def check_export(binary, root):
    world = setup(root)
    before = snapshot(world)
    output = root / "export.zip"
    output.write_bytes(b"preexisting destination")
    result = run(binary, root, "export", "demo", "Alpha", "export.zip")
    size = output.stat().st_size
    success(result, f"exported Alpha -> export.zip ({size} bytes, {len(FILES)} files)\n")
    content = archive_contents(output)
    assert snapshot(world) == before
    assert sorted(p.name for p in root.iterdir()) == ["export.zip", "instances"]
    return content


def check_backup(binary, root, label, suffix):
    world = setup(root)
    before_world = snapshot(world)
    args = ["backup", "demo", "Alpha"]
    if label is not None:
        args += ["--label", label]
    before = time.time_ns()
    result = run(binary, root, *args)
    after = time.time_ns()
    assert result[0] == 0 and result[2] == b"", result
    pattern = rb"backup: (Alpha-([0-9]{8}-[0-9]{6})" + re.escape(suffix.encode()) + rb"\.zip)\n"
    match = re.fullmatch(pattern, result[1])
    assert match, result
    name = match[1].decode()
    timestamp = datetime.strptime(match[2].decode(), "%Y%m%d-%H%M%S").replace(tzinfo=timezone.utc)
    assert before // 1_000_000_000 <= int(timestamp.timestamp()) <= after // 1_000_000_000
    backups = world.parent.parent / "backups"
    assert sorted(p.name for p in backups.iterdir()) == [name]
    archive = backups / name
    stat = archive.stat()
    # Filesystem clock granularity need not match process wall-clock sampling;
    # created_ms is validated against the actual stat below, without tolerance.
    content = archive_contents(archive)
    assert snapshot(world) == before_world
    # Each field is independently sourced from disk, not normalized for parity.
    created_ms = stat.st_mtime_ns // 1_000_000
    success(run(binary, root, "backups", "demo"),
            f"{name}\tAlpha\t{created_ms}\t{stat.st_size}\n")
    expected = {"backups": [{"file": name, "world": "Alpha", "created_ms": created_ms,
                              "size_bytes": stat.st_size}]}
    success(run(binary, root, "backups", "demo", "--json"),
            json.dumps(expected, separators=(",", ":")) + "\n")
    # Controlled subsecond mtime establishes that created_ms comes from stat,
    # not from the generated filename. This comparison has no clock tolerance.
    controlled_ns = EPOCH * 1_000_000_000 + 123_000_000
    os.utime(archive, ns=(controlled_ns, controlled_ns))
    observed_ms = archive.stat().st_mtime_ns // 1_000_000
    assert observed_ms == EPOCH * 1000 + 123
    success(run(binary, root, "backups", "demo"),
            f"{name}\tAlpha\t{observed_ms}\t{stat.st_size}\n")
    # Only the already parsed and wall-clock-validated timestamp is abstracted.
    normalized = f"backup: Alpha-<validated-UTC>{suffix}.zip\n"
    assert result[1] == f"backup: {name}\n".encode()
    return normalized, content


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["rust", "zig"]:
        parser.add_argument("--" + name, required=True)
        parser.add_argument("--" + name + "-sha256", required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="jmcl-world-cli-") as tmp:
        base = Path(tmp)
        binaries = [verified_copy(getattr(args, name), getattr(args, name + "_sha256"),
                                  base / (name + (".exe" if os.name == "nt" else "")))
                    for name in ["zig", "rust"]]
        results = []
        for index, binary in enumerate(binaries):
            root = base / f"export-{index}"
            root.mkdir()
            results.append(check_export(binary, root))
        assert results[0] == results[1]
        for case, (label, suffix) in enumerate([(None, ""), ("", ""), ("!!!", ""),
                                               ("night run!", "-night-run"), ("a" * 50, "-" + "a" * 40)]):
            results = []
            for index, binary in enumerate(binaries):
                root = base / f"backup-{case}-{index}"
                root.mkdir()
                results.append(check_backup(binary, root, label, suffix))
            assert results[0] == results[1]
        # Error output is compared verbatim, without paths/timestamps rewriting.
        for case, command in enumerate([("export", "demo", "Missing", "absent.zip"),
                                        ("export", "demo", "Alpha", "missing/absent.zip"),
                                        ("backup", "demo", "Missing")]):
            results = []
            for index, binary in enumerate(binaries):
                root = base / f"error-{case}-{index}"
                root.mkdir()
                world = setup(root)
                before = snapshot(world)
                results.append(run(binary, root, *command))
                assert snapshot(world) == before
                assert not (root / "absent.zip").exists()
                assert not (root / "missing").exists()
            assert results[0] == results[1] and results[0][0] == 1 and not results[0][1], results
    print("world CLI archive corpus: 9 paired scenarios passed; 48 command results checked")


if __name__ == "__main__":
    main()
