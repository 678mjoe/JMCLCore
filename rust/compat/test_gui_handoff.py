#!/usr/bin/env python3
"""Pure documentation/source checks; never invoke Core, Java or a transport.

These checks bind ordinary GUI instructions to reviewed Zig/Rust source and
independently derived UUID vectors, and keep the acceptance map consistent
with the existing URL decision. They are not executable RPC acceptance.
"""
import hashlib
import json
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]


def offline_uuid(name):
    digest = bytearray(hashlib.md5(("OfflinePlayer:" + name).encode("utf-8")).digest())
    digest[6] = (digest[6] & 0x0f) | 0x30
    digest[8] = (digest[8] & 0x3f) | 0x80
    return digest.hex()


class GuiHandoffContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = (ROOT / "docs/gui-handoff.md").read_text(encoding="utf-8")

    def test_documented_uuid_vectors(self):
        for name, expected in (
            ("Notch", "b50ad385829d3141a2167e7d7539ba7f"),
            ("Steve", "5627dd98e6be3c21b8a8e92344183641"),
        ):
            with self.subTest(name=name):
                self.assertEqual(offline_uuid(name), expected)
                self.assertIn(f"`{name}` → `{expected}`", self.docs)
                raw = hashlib.md5(("OfflinePlayer:" + name).encode()).hexdigest()
                self.assertNotEqual(raw, expected)

    def test_documented_bits_match_both_reviewed_implementations(self):
        sources = [
            self.docs,
            (ROOT / "rust/src/launch/offline_uuid.rs").read_text(encoding="utf-8"),
            (ROOT / "src/cli/launch.zig").read_text(encoding="utf-8"),
        ]
        for source in sources:
            self.assertIn('"OfflinePlayer:"', source)
            self.assertIn("digest[6] = (digest[6] & 0x0f) | 0x30", source)
            self.assertIn("digest[8] = (digest[8] & 0x3f) | 0x80", source)
        self.assertIn("UTF-8", self.docs)
        self.assertIn("32 lowercase hexadecimal characters, with no dashes", self.docs)

    def test_override_excludes_paths_not_policy_in_plan_and_execute(self):
        rust = (ROOT / "rust/src/launch/rpc.rs").read_text(encoding="utf-8")
        zig = (ROOT / "src/protocol/launch.zig").read_text(encoding="utf-8")
        self.assertEqual(rust.count("if p.java_override.is_some() && !p.java_paths.is_empty()"), 2)
        self.assertEqual(zig.count("if (input.java_override != null and input.java_paths.len != 0)"), 2)
        self.assertRegex(self.docs, r"mutually exclusive with non-empty\s+`java_paths`")
        self.assertNotRegex(self.docs, r"mutually exclusive with\s+`java_policy`")
        self.assertIn("A valid `java_policy` may accompany an override", self.docs)

    def test_preserved_native_trace_uses_override_with_local_policy(self):
        trace = ROOT / "rust/compat/frontend_bridge/traces/native-launch-frontend-shape-20261106/raw.jsonl"
        rows = [json.loads(line) for line in trace.read_text(encoding="utf-8").splitlines()]
        requests = [row for row in rows if row.get("method") == "launch.execute"]
        self.assertTrue(requests)
        for request in requests:
            params = request["params"]
            self.assertTrue(params["java_override"])
            self.assertEqual(params["java_policy"], "local")
            self.assertNotIn("java_paths", params)

    def test_acceptance_map_keeps_named_url_deviation_accepted(self):
        decision = (ROOT / "rust/compat/url-compatibility-decision.md").read_text(encoding="utf-8")
        acceptance = (ROOT / "docs/rewrite/candidate-acceptance-20261107.md").read_text(encoding="utf-8")
        cases = json.loads((ROOT / "rust/tests/fixtures/url_construction_cases.json").read_text(encoding="utf-8"))
        expected = {"unicode_path", "dot_segments", "escaped_dot", "raw_ff_path", "raw_ff_query", "surrogate_path"}
        self.assertEqual({row["name"] for row in cases if row["scope"] == "COMPAT-URL-001"}, expected)
        self.assertEqual(len(cases), 32)
        self.assertEqual(sum(row["scope"] == "strict-target-control" for row in cases), 8)
        self.assertEqual(sum(row["scope"] == "construction-only-diagnostic" for row in cases), 18)
        accepted = acceptance.split("### Accepted URL deviation, not an implementation blocker\n", 1)[1].split("### Remaining work\n", 1)[0]
        for name in expected:
            self.assertIn(f"`{name}`", decision)
            self.assertIn(f"`{name}`", accepted)
        self.assertIn("not a replacement\nblocker requiring a dedicated byte-preserving transport", accepted)
        self.assertIn("not a Zig defect or wire-parity claim", accepted)
        self.assertIn("Do not extend the decision into a blanket mismatch or security-policy waiver", accepted)
        remaining = acceptance.split("### Remaining work\n", 1)[1].split("## 6.", 1)[0]
        self.assertNotIn("raw URL continuation", remaining)


if __name__ == "__main__":
    unittest.main()
