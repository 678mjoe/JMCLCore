#!/usr/bin/env python3
"""Pure guard regressions. No compiler, Core, Java, network, or external process."""
import os
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch
import run


class Guards(unittest.TestCase):
    def test_fixed_scope(self):
        self.assertEqual(len(run.SELECTORS), 6)
        self.assertEqual(len(set(run.SELECTORS)), 6)
        self.assertEqual(len(run.WRAPPERS), 2)
        self.assertNotIn("java_detect_returns_object", run.SELECTORS)
        self.assertEqual(len(run.HOSTS), 4)

    def test_exact_commands(self):
        for name in run.SELECTORS:
            command = run.test_command(Path("owned/Cargo.toml"), name=name)
            self.assertEqual(command[-5:], [name, "--", "--exact", "--nocapture", "--test-threads=1"])
            self.assertIn("--offline", command)
            self.assertIn("--locked", command)
            self.assertEqual(command[command.index("--test") + 1], "rpc_smoke")
            for forbidden in ("--all-features", "--include-ignored", "--skip", "--all-targets"):
                self.assertNotIn(forbidden, command)

    def test_no_unbounded_execution_command(self):
        for name in (None, "java_detect_returns_object", "", "handshake"):
            with self.assertRaises(RuntimeError):
                run.test_command(Path("owned/Cargo.toml"), name=name)

    def test_inventory(self):
        for windows in (False, True):
            names = run.SELECTORS + run.UNRUN_INVENTORY + (("local_child_has_no_console_and_keeps_rpc_pipes",) if windows else ())
            text = "\n".join(name + ": test" for name in names)
            self.assertEqual(run.verify_inventory(text, windows), list(names))
            for bad in (text + "\nunknown: test", text + "\n" + names[0] + ": test", text.replace(names[0] + ": test", ""), ""):
                with self.assertRaises(RuntimeError):
                    run.verify_inventory(bad, windows)

    def test_exact_results(self):
        for name in run.SELECTORS:
            good = f"running 1 test\ntest {name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.01s\n"
            run.verify_single(good, name)
            bad_outputs = [good.replace("running 1", "running 0"), good.replace("running 1 test", "running 2 tests"),
                           good.replace("1 passed", "0 passed"), good.replace("0 ignored", "1 ignored"),
                           good.replace("0 failed", "1 failed"), good.replace("... ok", "... ignored"),
                           good.replace(name, "java_detect_returns_object"), good + good, ""]
            for bad in bad_outputs:
                with self.assertRaises(RuntimeError):
                    run.verify_single(bad, name)

    def test_host_and_versions_fail_closed(self):
        for label, (system, machine, host) in run.HOSTS.items():
            rust = f"release: 1.99.1\nhost: {host}\n"
            self.assertEqual(run.verify_host(label, rust, "1.4.4\n", system, machine), host)
            for args in [(label, rust.replace("1.99.1", "1.99.0"), "1.4.4", system, machine),
                         (label, rust, "latest", system, machine), (label, rust, "1.4.4", system, "other"),
                         (label, rust.replace(host, "other"), "1.4.4", system, machine)]:
                with self.assertRaises(RuntimeError):
                    run.verify_host(*args)

    def test_architectures(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "fixture"
            fixtures = {}
            elf = bytearray(128); elf[:6] = b"\x7fELF\x02\x01"
            struct.pack_into("<HH", elf, 16, 3, 62)
            fixtures["x86_64-unknown-linux-gnu"] = elf
            for host, cpu in (("aarch64-apple-darwin", 0x0100000C), ("x86_64-apple-darwin", 0x01000007)):
                macho = bytearray(128); macho[:4] = b"\xcf\xfa\xed\xfe"
                struct.pack_into("<I", macho, 4, cpu); struct.pack_into("<I", macho, 12, 2)
                fixtures[host] = macho
            pe = bytearray(160); pe[:2] = b"MZ"; struct.pack_into("<I", pe, 60, 80)
            pe[80:84] = b"PE\0\0"; struct.pack_into("<H", pe, 84, 0x8664)
            struct.pack_into("<HH", pe, 102, 2, 0x20B)
            fixtures["x86_64-pc-windows-msvc"] = pe
            for host, data in fixtures.items():
                path.write_bytes(data); self.assertEqual(run.verify_architecture(path, host)["sha256"], run.digest(path))
                for other in set(fixtures) - {host}:
                    with self.assertRaises(RuntimeError):
                        run.verify_architecture(path, other)
            for data in (b"#!/bin/sh\nexit 0\n" * 8, b"MZ", b"\0" * 128):
                path.write_bytes(data)
                with self.assertRaises(RuntimeError):
                    run.verify_architecture(path, "x86_64-pc-windows-msvc")
            pe[102] = 0; path.write_bytes(pe)
            with self.assertRaises(RuntimeError):
                run.verify_architecture(path, "x86_64-pc-windows-msvc")

    def test_owned_temporary_root_and_overrides(self):
        with tempfile.TemporaryDirectory() as root, patch.dict(os.environ, {"JMCL_CORE": "zig", "JMCL_OTHER": "bad", "CARGO_BUILD_TARGET": "cross", "RUSTFLAGS": "bad"}):
            root = Path(root)
            env = run.clean_environment(root / "one", root / "target")
            self.assertEqual({env[k] for k in ("TMPDIR", "TMP", "TEMP")}, {str((root / "one").resolve())})
            self.assertNotIn("JMCL_CORE", env); self.assertNotIn("JMCL_OTHER", env)
            self.assertNotIn("CARGO_BUILD_TARGET", env); self.assertNotIn("RUSTFLAGS", env)
            with self.assertRaises(FileExistsError):
                run.clean_environment(root / "one", root / "target")
            self.assertNotEqual(env["TMPDIR"], run.clean_environment(root / "two", root / "target")["TMPDIR"])

    def test_source_pin_and_dirty_rejections_without_git(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            for values in (["wrong"], [run.CORE_COMMIT, "wrong"], [run.CORE_COMMIT, run.CORE_TREE, " M rust/src/main.rs"],
                           [run.CORE_COMMIT, run.CORE_TREE, "", "injected.rs"]):
                with patch.object(run, "output", side_effect=values), self.assertRaises(RuntimeError):
                    run.verify_source(root, run.CORE_COMMIT, run.CORE_TREE)

    def test_prepare_byte_copies_only(self):
        with tempfile.TemporaryDirectory() as root:
            front = Path(root)
            (front / "src-tauri/src").mkdir(parents=True)
            (front / "src-tauri/tests").mkdir()
            for name in run.MODULES:
                (front / "src-tauri/src" / name).write_bytes(name.encode())
            (front / "src-tauri/tests/rpc_smoke.rs").write_bytes(b"unchanged tests")
            harness = run.prepare(front)
            for name in run.MODULES:
                self.assertEqual((harness / "src" / name).read_bytes(), name.encode())
            self.assertEqual((harness / "tests/rpc_smoke.rs").read_bytes(), b"unchanged tests")
            with self.assertRaises(RuntimeError):
                run.prepare(front)

    def test_recorder_pure_fake_success_and_failure(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root); recorder = run.Recorder(root)
            fake = type("Result", (), {"stdout": b"owned fake output\r\n", "returncode": 0})()
            with patch.object(run.subprocess, "run", return_value=fake) as child:
                self.assertEqual(recorder.run("fake", ["never-executed", "--exact"], root, {"TMPDIR": str(root)}), "owned fake output\n")
                self.assertEqual(child.call_count, 1)
                self.assertEqual(recorder.records[0]["command"], ["never-executed", "--exact"])
                self.assertEqual(recorder.records[0]["log_sha256"], run.digest(root / "01-fake.log"))
            fake.returncode = 9
            with patch.object(run.subprocess, "run", return_value=fake), self.assertRaises(RuntimeError):
                recorder.run("fake-failure", ["never-executed"], root, {"TMPDIR": str(root)})

    def test_raw_invalid_utf8_and_timeout_are_preserved(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root); recorder = run.Recorder(root)
            raw = b"raw\r\n\xff\x00"
            fake = type("Result", (), {"stdout": raw, "returncode": 0})()
            with patch.object(run.subprocess, "run", return_value=fake), self.assertRaises(UnicodeDecodeError):
                recorder.run("invalid", ["never-executed"], root, {"TMPDIR": str(root)})
            self.assertEqual((root / "01-invalid.log").read_bytes(), raw)
            self.assertEqual(recorder.records[0]["log_sha256"], run.digest(root / "01-invalid.log"))
            timeout = run.subprocess.TimeoutExpired(["never-executed"], 1, output=raw)
            with patch.object(run.subprocess, "run", side_effect=timeout), self.assertRaises(run.subprocess.TimeoutExpired):
                recorder.run("timeout", ["never-executed"], root, {"TMPDIR": str(root)}, timeout=1)
            self.assertEqual((root / "02-timeout.log").read_bytes(), raw)
            self.assertTrue(recorder.records[1]["timed_out"])
            self.assertIsNone(recorder.records[1]["returncode"])


if __name__ == "__main__":
    unittest.main()
