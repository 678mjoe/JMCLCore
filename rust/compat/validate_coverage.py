#!/usr/bin/env python3
"""Validate the complete pinned method scope without pretending it is coverage."""
import argparse
import json
from pathlib import Path
import re
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle-source", type=Path, required=True)
    args = parser.parse_args()
    here = Path(__file__).resolve().parent
    methods = json.loads((here / "methods.json").read_text())
    coverage = json.loads((here / "coverage.json").read_text())
    docs = json.loads((here.parent.parent / "docs/rewrite/rpc-methods.json").read_text())
    revision = subprocess.check_output(["git", "-C", str(args.oracle_source), "rev-parse", "HEAD"], text=True).strip()
    assert revision == methods["oracle_commit"] == coverage["oracle_commit"] == docs["oracle_commit"], "Oracle pin drift"
    source = (args.oracle_source / "src/protocol/protocol.zig").read_text()
    actual = re.findall(r'std\.mem\.eql\(u8, request\.method, "([^"]+)"\)', source)
    assert len(actual) == len(set(actual)), "Duplicate oracle dispatch method"
    assert actual == methods["methods"] == docs["methods"], "Method inventory drift"
    rows = coverage["methods"]
    assert [row["method"] for row in rows] == actual, "Per-method evidence matrix drift"
    assert len(actual) == docs["method_count"]
    for row in rows:
        for key in ("source_inventory", "implementation", "success_cases", "invalid_cases", "error_cases", "event_cases", "disk_cases", "platform_evidence", "known_gaps", "compatible"):
            assert key in row, (row["method"], key)
        if row["compatible"]:
            assert not row["known_gaps"], (row["method"], "Claimed compatible with gaps")
            assert row["implementation"] != "not_started"
    assert set(methods["implemented"]).issubset(actual)
    assert set(methods.get("partially_implemented", {})).issubset(actual)
    if coverage["replacement_allowed"] or coverage["full_compatibility"]:
        assert all(row["compatible"] for row in rows), "Replacement claim before every method gate"
    print(json.dumps({"oracle_commit": revision, "inventoried_methods": len(actual), "compatible_methods_claimed": sum(row["compatible"] for row in rows), "replacement_allowed": coverage["replacement_allowed"]}))


if __name__ == "__main__":
    main()
