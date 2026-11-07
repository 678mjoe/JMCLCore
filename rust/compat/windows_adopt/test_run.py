import unittest
import contextlib
import io
import subprocess
import json
import base64
from run import MARKER, TEST, validate_result, validate_pe, execute

class Guards(unittest.TestCase):
    def test_output(self):
        good = b'running 1 test\n'+TEST.encode()+b'\n'+MARKER+b'\ntest result: ok. 1 passed; 0 failed; 0 ignored; 2 filtered out;\n'
        validate_result(0, good)
        for code, raw in [(1,good), (0,good.replace(MARKER,b'')), (0,good+MARKER), (0,good.replace(b'1 passed',b'0 passed')), (0,good.replace(b'0 ignored',b'1 ignored')), (0,good.replace(b'0 failed',b'1 failed')), (0,good.replace(TEST.encode(), b'other'))]:
            with self.assertRaises(RuntimeError):
                validate_result(code, raw)
    def test_timeout_preserves_raw_bytes(self):
        raw = b'partial\r\n\x00\xff'
        def timeout(*args, **kwargs):
            raise subprocess.TimeoutExpired(args[0], 1200, output=raw)
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            code, captured = execute(['fake'], '.', {}, runner=timeout)
        self.assertIsNone(code)
        self.assertEqual(captured, raw)
        record = json.loads(output.getvalue().removeprefix('RAW_COMMAND '))
        self.assertTrue(record['timed_out'])
        self.assertEqual(base64.b64decode(record['raw_base64']), raw)
        with self.assertRaises(RuntimeError):
            validate_result(code, captured)
    def test_pe_rejects_script_and_truncation(self):
        for data in (b'#!/bin/sh\n', b'MZ'+b'\0'*100):
            with self.assertRaises(RuntimeError):
                validate_pe(data)

if __name__ == '__main__':
    unittest.main()
