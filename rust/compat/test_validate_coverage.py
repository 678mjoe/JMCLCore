#!/usr/bin/env python3
"""Static-audit regression tests; no Core, JVM, provider or archive execution."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("validate_coverage", HERE / "validate_coverage.py")
validator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(validator)


class DispatchAuditTests(unittest.TestCase):
    def setUp(self):
        self.root = HERE.parent.parent
        self.audit = json.loads((HERE / "dispatch-audit.json").read_text())

    def rejects(self, fragment):
        with self.assertRaisesRegex(validator.AuditError, fragment):
            validator.validate_dispatch(self.root, self.audit)

    def test_current_review_is_static_not_runtime_evidence(self):
        # Even accidental subprocess use would fail this test.
        with patch.object(validator.subprocess, "check_output", side_effect=AssertionError("execution forbidden")):
            result = validator.validate_dispatch(self.root, self.audit)
        self.assertEqual(result["inventoried_names"], 106)
        self.assertEqual(result["static_handler_associations"], 105)
        self.assertEqual(result["blocked_not_implemented"], ["java.runtime.install"])
        self.assertEqual(result["core_requests_executed"], 0)
        self.assertFalse(result["runtime_coverage_claimed"])
        self.assertFalse(result["replacement_allowed"])

    def test_stale_source_digest_is_rejected(self):
        self.audit["source_sha256"]["rust/src/protocol.rs"] = "0" * 64
        self.rejects("Routing source changed")

    def test_missing_handler_association_is_rejected(self):
        self.audit["routes"][0]["source_links"][0]["anchor"] = "pub fn removed_handler("
        self.rejects("Missing pinned handler/re-export edge")

    def test_missing_protocol_call_is_rejected(self):
        self.audit["routes"][0]["protocol_anchor"] = "crate::removed::handle("
        self.rejects("Missing protocol edge")

    def test_reordered_routes_are_rejected(self):
        self.audit["routes"][0:2] = list(reversed(self.audit["routes"][0:2]))
        self.rejects("Protocol order changed")

    def test_missing_method_is_rejected(self):
        self.audit["routes"][0]["methods"].pop()
        self.rejects("Audit/inventory mismatch")

    def test_duplicate_association_is_rejected(self):
        self.audit["routes"][1]["methods"].append(self.audit["routes"][0]["methods"][0])
        self.rejects("Multiply associated method")

    def test_wrong_product_mapping_is_rejected(self):
        self.audit["routes"][1]["methods"][0] = "mods.adopt"
        self.rejects("Incorrect product expansion")

    def test_unpinned_family_selector_is_rejected(self):
        self.audit["routes"][1]["selector"]["family_path"] = "rust/src/absent.rs"
        self.rejects("Unpinned family selector")

    def test_install_cannot_be_relabelled_as_handler(self):
        self.audit["routes"][-1]["status"] = "static_handler_reviewed"
        self.rejects("Invalid fallback association")

    def test_other_method_cannot_be_assigned_blocked_fallback(self):
        self.audit["routes"][-1]["methods"] = ["invented.method"]
        self.rejects("Invalid fallback association")

    def test_blocked_install_cannot_claim_executed_evidence(self):
        self.audit["routes"][-1]["runtime_evidence"]["paths"] = ["rust/compat/java.py"]
        self.rejects("Blocked method given runtime evidence")

    def test_static_fixture_references_cannot_claim_execution(self):
        self.audit["routes"][0]["runtime_evidence"]["status"] = "passed"
        self.rejects("mislabeled as executed evidence")

    def test_missing_fixture_reference_is_rejected(self):
        self.audit["routes"][0]["runtime_evidence"]["paths"] = ["rust/tests/missing.rs"]
        self.rejects("Missing/outside audit file")

    def test_status_document_is_not_executable_fixture_source(self):
        self.audit["routes"][0]["runtime_evidence"]["paths"] = ["rust/compat/coverage.json"]
        self.rejects("Evidence must reference fixture source")

    def test_reference_cannot_escape_repository(self):
        self.audit["routes"][0]["runtime_evidence"]["paths"] = ["../external.rs"]
        self.rejects("Non-repository audit path")

    def test_linux_server_rpc_boundary_cannot_be_promoted(self):
        self.audit["routes"][2]["platform"] = "supported_desktop_targets"
        self.rejects("Server RPC platform boundary missing")

    def test_source_content_change_invalidates_review(self):
        # Copy only text dependencies into an isolated fixture; never run them.
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = set(self.audit["source_sha256"])
            paths.add("rust/compat/methods.json")
            for route in self.audit["routes"]:
                paths.update(route["runtime_evidence"]["paths"])
            for relative in paths:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((self.root / relative).read_bytes())
            protocol = root / "rust/src/protocol.rs"
            protocol.write_text(protocol.read_text().replace("crate::auth::handle(", "crate::absent::handle("))
            with self.assertRaisesRegex(validator.AuditError, "Routing source changed"):
                validator.validate_dispatch(root, copy.deepcopy(self.audit))


if __name__ == "__main__":
    unittest.main()
