#!/usr/bin/env python3
"""Pure guard controls; no Core process, service, Java, network or installation."""
import base64
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('discovery', Path(__file__).with_name('run.py'))
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)

class Guards(unittest.TestCase):
    def test_require_remains_active(self):
        with self.assertRaises(RuntimeError):
            r.require(False, 'negative control')

    def test_optimized_entry_rejected(self):
        for flag in ('-O', '-OO'):
            p = subprocess.run([sys.executable, flag, str(r.BASE / 'run.py'), '--help'], capture_output=True)
            self.assertNotEqual(p.returncode, 0)
            self.assertIn(b'Python optimization disables acceptance gates', p.stderr)

    def test_native_host_and_toolchain(self):
        for label, (system, machine, target) in r.HOSTS.items():
            version = f'release: {r.RUST_VERSION}\nhost: {target}'
            self.assertEqual(r.verify_host(label, version, system, machine), target)
            for wrong in (version.replace(r.RUST_VERSION, '1.0.0'), version.replace(target, 'wrong-host')):
                with self.assertRaises(RuntimeError):
                    r.verify_host(label, wrong, system, machine)
            with self.assertRaises(RuntimeError):
                r.verify_host(label, version, system, 'wrong-architecture')

    def test_environment_isolated(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict(os.environ, {'JMCL_CORE': 'wrong', 'JMCL_ANY': 'secret', 'CARGO_BUILD_TARGET': 'wrong', 'RUSTFLAGS': 'wrong', 'CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS': 'true'}):
                env = r.clean_environment(Path(tmp) / 'temp', Path(tmp) / 'target')
            for key in ('JMCL_CORE', 'JMCL_ANY', 'CARGO_BUILD_TARGET', 'RUSTFLAGS', 'CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS'):
                self.assertNotIn(key, env)
            self.assertEqual(env['RUSTUP_TOOLCHAIN'], '1.99.1')

    def test_header_controls(self):
        fixtures = {}
        elf = bytearray(64); elf[:6] = b'\x7fELF\x02\x01'; struct.pack_into('<HH', elf, 16, 3, 62)
        fixtures['x86_64-unknown-linux-gnu'] = elf
        for target, cpu in [('aarch64-apple-darwin', 0x0100000C), ('x86_64-apple-darwin', 0x01000007)]:
            data = bytearray(64); data[:4] = b'\xcf\xfa\xed\xfe'; struct.pack_into('<I', data, 4, cpu); struct.pack_into('<I', data, 12, 2)
            fixtures[target] = data
        pe = bytearray(128); pe[:2] = b'MZ'; struct.pack_into('<I', pe, 60, 64); pe[64:68] = b'PE\0\0'; struct.pack_into('<H', pe, 68, 0x8664); struct.pack_into('<H', pe, 86, 2); struct.pack_into('<H', pe, 88, 0x20B)
        fixtures['x86_64-pc-windows-msvc'] = pe
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'fixture'
            for target, data in fixtures.items():
                path.write_bytes(data)
                self.assertIn('sha256', r.verify_architecture(path, target))
                for other in fixtures:
                    if other != target:
                        with self.assertRaises(RuntimeError):
                            r.verify_architecture(path, other)
            for data in (b'', b'#!not-a-real-executable\n' * 5):
                path.write_bytes(data)
                for target in fixtures:
                    with self.assertRaises(RuntimeError):
                        r.verify_architecture(path, target)

    def test_raw_bytes_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            record = r.Recorder(root)
            expected = b'a\r\nb\xff\x00\n'
            result = record.run('bytes-control', [sys.executable, '-c', 'import os; os.write(1, ' + repr(expected) + ')'], root, os.environ, 10)
            self.assertEqual(result, expected)
            self.assertEqual((root / 'bytes-control.raw').read_bytes(), expected)
            self.assertEqual(base64.b64decode(record.records[0]['raw_base64']), expected)
            self.assertEqual(record.records[0]['raw_sha256'], r.digest(root / 'bytes-control.raw'))

    def test_recorder_failure_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); recorder = r.Recorder(root)
            with self.assertRaises(RuntimeError):
                recorder.run('fail', [sys.executable, '-c', 'raise SystemExit(7)'], root, os.environ, 10)
            self.assertEqual(recorder.records[0]['returncode'], 7)

if __name__ == '__main__':
    unittest.main()
