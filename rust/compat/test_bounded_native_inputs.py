#!/usr/bin/env python3
"""Pure guard unit tests; no compiler or test subprocess is invoked."""
import json
import unittest
from unittest.mock import patch
from pathlib import Path
import bounded_native_inputs as guard

class GuardTests(unittest.TestCase):
    def setUp(self):
        self.groups = json.loads(guard.ALLOWLIST.read_text())
    def test_manifest_drift_rejected(self):
        with self.assertRaises(ValueError): guard.verified_allowlist(guard.ALLOWLIST.read_bytes() + b" ")
    def test_reviewed_manifest_accepted(self):
        self.assertEqual(guard.verified_allowlist(guard.ALLOWLIST.read_bytes()),self.groups)
    def test_source_tree_and_commit_are_both_pinned(self):
        for output in [['wrong'], [guard.SOURCE_COMMIT,'wrong'], [guard.SOURCE_COMMIT,guard.SOURCE_TREE,' M rust/src/lib.rs']]:
            with patch.object(guard,'checked',side_effect=output):
                with self.assertRaises(ValueError): guard.verify_source(Path('source'))
    def test_exact_clean_source_and_compiler_accepted(self):
        output=[guard.SOURCE_COMMIT,guard.SOURCE_TREE,'','cargo 1.99.1 (test)','rustc 1.99.1 (test)']
        with patch.object(guard,'checked',side_effect=output): guard.verify_source(Path('source'))
    def test_final_source_failure_prints_no_success(self):
        with patch.object(guard,'verify_source',side_effect=ValueError('drift')), patch('builtins.print') as output:
            with self.assertRaises(ValueError): guard.report_after_verification(Path('source'),{'passed':25})
            output.assert_not_called()
    def test_exact_platform_counts(self):
        for platform, count in [('linux',25),('darwin',23),('win32',20)]:
            self.assertEqual(len(guard.names_for(platform,self.groups)),count)
    def test_unknown_host_rejected(self):
        with self.assertRaises(ValueError): guard.names_for('unknown',self.groups)
    def test_duplicate_manifest_rejected(self):
        self.groups['common'][0] = self.groups['common'][1]
        with self.assertRaises(ValueError): guard.names_for('linux',self.groups)
    def test_zero_or_missing_inventory_rejected(self):
        for output in ['', 'other: test\n']:
            with self.assertRaises(ValueError): guard.validate_inventory(output,['wanted'])
    def test_duplicate_inventory_rejected(self):
        with self.assertRaises(ValueError): guard.validate_inventory('wanted: test\nwanted: test',['wanted'])
    def test_exact_inventory_accepted(self):
        guard.validate_inventory('wanted: test\nother: test',['wanted'])
    def test_zero_ignored_or_failed_run_rejected(self):
        for code, output in [(0,'0 passed; 0 failed; 0 ignored;'),(0,'0 passed; 0 failed; 1 ignored;'),(1,'1 passed; 0 failed; 0 ignored;')]:
            with self.assertRaises(ValueError): guard.validate_result(code,output,'wanted')
    def test_exact_identity_required(self):
        with self.assertRaises(ValueError): guard.validate_result(0,'test other ... ok\n1 passed; 0 failed; 0 ignored;','wanted')
    def test_exact_success_accepted(self):
        guard.validate_result(0,'test wanted ... ok\n1 passed; 0 failed; 0 ignored;','wanted')

if __name__ == '__main__': unittest.main()
