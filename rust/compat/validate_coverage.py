#!/usr/bin/env python3
"""Validate the complete pinned method scope without pretending it is coverage."""
import argparse
import json
import hashlib
from pathlib import Path
import re
import subprocess



class AuditError(ValueError):
    """A reviewed association or source/evidence reference has drifted."""


def require(condition, message):
    if not condition:
        raise AuditError(message)


def repository_file(root, relative):
    path = Path(relative)
    require(not path.is_absolute() and ".." not in path.parts,
            f"Non-repository audit path: {relative}")
    resolved = (root / path).resolve()
    require(resolved.is_relative_to(root.resolve()) and resolved.is_file(),
            f"Missing/outside audit file: {relative}")
    return resolved


def validate_dispatch(root, audit=None):
    """Validate a manually reviewed, digest-bound map; never execute Core.

    This is deliberately not a Rust parser or an assertion that any request
    succeeds. Digests make an earlier source review stale on *any* file change.
    Selectors and call/re-export anchors expose the reviewed routing reasoning.
    Evidence references mean existing related fixture source, not execution.
    """
    root = Path(root)
    compat = root / "rust/compat"
    if audit is None:
        audit = json.loads((compat / "dispatch-audit.json").read_text())
    inventory = json.loads((compat / "methods.json").read_text())
    require(audit["schema_version"] == 1, "Unsupported dispatch audit schema")
    require(audit["oracle_commit"] == inventory["oracle_commit"], "Audit oracle pin drift")
    require(audit["execution_policy"] == "static_only_no_core_invocation", "Unsafe audit policy")
    require(audit["full_compatibility"] is False and audit["replacement_allowed"] is False,
            "Static audit cannot authorize replacement")
    require(audit.get("source_digest_encoding") == "utf8_crlf_to_lf", "Unsupported source digest encoding")
    sources, normalized = {}, []
    for path, expected in audit["source_sha256"].items():
        raw = repository_file(root, path).read_bytes()
        # Git may check text out as CRLF on Windows. Normalize only paired
        # CRLF, never whitespace, bare CR, tokens, or string escape sequences.
        data = raw.replace(b"\r\n", b"\n")
        if data != raw:
            normalized.append(path)
        require(hashlib.sha256(data).hexdigest() == expected,
                f"Routing source changed; review associations before repinning: {path}")
        sources[path] = data.decode("utf-8")
    protocol = sources.get("rust/src/protocol.rs", "")
    require(protocol, "Protocol dispatcher must be pinned")
    ids, seen, required_sources = set(), set(), {"rust/src/protocol.rs"}
    previous_position = -1
    routed, blocked = [], []
    for route in audit["routes"]:
        name = route["id"]
        require(name not in ids, f"Duplicate route: {name}")
        ids.add(name)
        methods = route["methods"]
        require(methods and len(methods) == len(set(methods)), f"Empty/duplicate methods: {name}")
        require(not seen.intersection(methods), f"Multiply associated method: {name}")
        seen.update(methods)
        anchor = route["protocol_anchor"]
        require(anchor in protocol, f"Missing protocol edge: {name}")
        position = protocol.index(anchor)
        require(position >= previous_position, f"Protocol order changed: {name}")
        previous_position = position
        require(route["source_links"], f"Missing handler/re-export links: {name}")
        for link in route["source_links"]:
            path = link["path"]
            required_sources.add(path)
            require(path in sources and link["anchor"] in sources[path],
                    f"Missing pinned handler/re-export edge: {name}: {path}")
        selector = route["selector"]
        path = selector["path"]
        required_sources.add(path)
        require(path in sources, f"Unpinned selector: {name}")
        source = sources[path]
        kind = selector["kind"]
        if kind == "literals":
            for method in methods:
                require(json.dumps(method) in source, f"Missing method literal: {name}: {method}")
        elif kind == "product":
            family_path = selector["family_path"]
            required_sources.add(family_path)
            require(family_path in sources, f"Unpinned family selector: {name}")
            for family in selector["families"]:
                require(json.dumps(family) in sources[family_path], f"Missing family: {name}: {family}")
            for op in selector["operations"]:
                require(json.dumps(op) in source, f"Missing operation: {name}: {op}")
            for guard in selector["anchors"]:
                require(guard in source, f"Missing product guard: {name}")
            expanded = {selector["prefix"] + family + "." + op
                        for family in selector["families"] for op in selector["operations"]}
            require(set(methods) == expanded, f"Incorrect product expansion: {name}")
        else:
            require(kind == "inventory_fallback" and methods == ["java.runtime.install"]
                    and route["status"] == "blocked_not_implemented"
                    and anchor == "method if known_method(method) => error(",
                    f"Invalid fallback association: {name}")
        evidence = route["runtime_evidence"]
        if route["status"] == "blocked_not_implemented":
            require(kind == "inventory_fallback" and not evidence["paths"]
                    and evidence["status"] == "not_executed_blocked"
                    and route["platform"] == "all_targets_not_implemented",
                    f"Blocked method given runtime evidence: {name}")
            blocked.extend(methods)
        else:
            require(route["status"] == "static_handler_reviewed" and kind != "inventory_fallback",
                    f"Unsupported static status: {name}")
            require(evidence["status"] == "referenced_not_run" and evidence["paths"]
                    and evidence["scope"] == "related_group_fixtures_not_per_method_or_platform_completion",
                    f"Static association mislabeled as executed evidence: {name}")
            for evidence_path in evidence["paths"]:
                repository_file(root, evidence_path)
                require(evidence_path.endswith((".py", ".rs")),
                        f"Evidence must reference fixture source, not a status document: {name}")
            if any(method.startswith("server.") for method in methods):
                require(route["platform"] == "linux_rpc_other_targets_reject",
                        f"Server RPC platform boundary missing: {name}")
            else:
                require(route["platform"] == "supported_desktop_targets", f"Invalid platform: {name}")
            routed.extend(methods)
    require(seen == set(inventory["methods"]),
            f"Audit/inventory mismatch: missing={sorted(set(inventory['methods'])-seen)}, extra={sorted(seen-set(inventory['methods']))}")
    require(set(sources) == required_sources, "Unused source pin or unpinned routing dependency")
    require(blocked == ["java.runtime.install"], "Blocked installation boundary changed")
    return {"audit_mode": "static_only", "inventoried_names": len(seen),
            "static_handler_associations": len(routed), "blocked_not_implemented": blocked,
            "core_requests_executed": 0, "runtime_coverage_claimed": False,
            "replacement_allowed": False, "crlf_sources_normalized": normalized}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle-source", type=Path)
    parser.add_argument("--static-only", action="store_true",
                        help="Validate reviewed routing/evidence references without executing Core or git")
    args = parser.parse_args()
    here = Path(__file__).resolve().parent
    audit_summary = validate_dispatch(here.parent.parent)
    if args.static_only:
        print(json.dumps(audit_summary))
        return
    if args.oracle_source is None:
        parser.error("--oracle-source is required unless --static-only is supplied")
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
    print(json.dumps({"oracle_commit": revision, "inventoried_methods": len(actual), "compatible_methods_claimed": sum(row["compatible"] for row in rows), "replacement_allowed": coverage["replacement_allowed"], "dispatch_audit": audit_summary}))


if __name__ == "__main__":
    main()
