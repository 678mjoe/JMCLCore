# Frozen release Core discovery: four-platform result

Run: https://github.com/678mjoe/JMCLCore/actions/runs/42390179188

Status: completed / success. Exactly four native jobs; each passed present-adjacent and absent-sidecar, for eight scenario passes. No retries or corrective CI commits were required.

Executed harness commit: d596f19a408a8daf4e003998f8116882c7fdc399
Harness tree: 9cf5b2283d6a4c439e715737d84d4cb36baa19b1
Core: bdd6cb3e55d69cb9e0e02037d251635e289207a8 / tree 0e8e901bc587cf3c6d7dc1ba78821c4701785e20
Frontend: 9c0fd81ce8109c1e35229560fee31308ca114ef5 / tree 48ea91981bdb7b375bca8e9bc47738b6c3dde4d1

## Native results

- macos-15 (aarch64-apple-darwin): 2/2; four original subprocess streams verified.
  Core SHA256: 59d2ad342d0c66ac5b7c1fa7a5fd328604a349da365e569c5b9f57e6694ae582
  Release probe SHA256: e99ab8982d933d496a3a06aabf80f1d008dc397bbccf6afb2031da3f882b3afc
  Report SHA256: fb7c572a5c835c7465f5d453414ea88335eedc1cf0f79c601fd5e940169a7d4b
- ubuntu-24.04 (x86_64-unknown-linux-gnu): 2/2; four original subprocess streams verified.
  Core SHA256: 2fd1612ba515063647b33007d6b23113fb5526e5f42ef8c8abd8657930108714
  Release probe SHA256: 022cf80216d042910bae0f797cab696116ba4ae45a0ed752454dd8028c245ca6
  Report SHA256: f7f4896da325d1c50238b53f9b40a5dc7db38e162d92db0c9171475deff43317
- windows-2025 (x86_64-pc-windows-msvc): 2/2; four original subprocess streams verified.
  Core SHA256: 9002847d63745b6be495f3e7190681358c2b5d3dc2784637a93f4dd83ff5c018
  Release probe SHA256: b7d36fca28ea885a6f10dd50c7141262ffa44641e77c256d750fe666b3bb6051
  Report SHA256: 32fa76e74f6525bf54fd1a6b059ff5e9f3e940323995c754d9c1dabf18af167f
- macos-15-intel (x86_64-apple-darwin): 2/2; four original subprocess streams verified.
  Core SHA256: 579593e7e07fbaa4a3637183871d6b4bfe6d95be8b46bd0903bb617505bb0b04
  Release probe SHA256: 8e87c12d0f2dc5ddcb37e8f5a5920fa3bd6c50e4ce118950160e73874ae67534
  Report SHA256: 994122ac29c9cbdd6897944653662217ee74d6ec83dae1d98f0ef445ee2e535c

## What was verified

- Actual ELF64 x86_64, thin Mach-O64 x86_64/aarch64, or PE32+ x86_64 binary headers; pinned native Rust 1.99.1.
- Clean, fixed source commits/trees and all 818 Core plus 292 frontend file hashes matched across all runners and the local reference. Harness/workflow hashes matched the independently reviewed source.
- The byte-identical production session/transport modules execute in a truly release-built probe with debug_assertions false. Core is a frozen native debug build; this is not a claim that Core itself was release-profile-built.
- Without JMCL_CORE or explicit binary_path, real resolve_core_binary(None) returns the exact adjacent Core path and frozen binary hash, then real Session completes handshake, ping and close.
- Moving only the owned sidecar outside all eight search ancestors produces the exact missing-Core error. Ten owned nesting levels isolate host fallback directories; launch CWD is separate. macOS uses simulated Contents/MacOS.
- Seven pure guard tests pass both normally and under optimized Python on each runner. The runtime entry separately refuses optimized execution. These are guard controls, not extra Core scenarios.
- Four verified report files and sixteen original subprocess pipe byte streams retained. Base64 decoded bytes match recorded length and SHA256. Actions-decoded job logs are explicitly labelled decoded logs, not raw subprocess output. Windows report CRLF is restored only when the full recorded byte hash matches.

## Limits

This verifies release resolver/native Session behavior only. It does not verify Tauri IPC, UI, real signed bundles/installers, production release packaging, Java, real accounts/services, archive workflows, or overall replacement readiness. Original workflows, production main and sidecar pins remain unchanged. No new paid/private runners, releases or artifact uploads were used.

## Retained evidence

The source-and-evidence backup contains all 818 fixed Core and 292 fixed frontend source files, the nine executed harness files, four reconstructed report files, sixteen original subprocess pipe streams, decoded Actions job logs, API status snapshots, checksum indexes and a standalone verifier. It contains no built Core or probe binaries. No binary Actions artifact was uploaded. Original Actions job logs remain linked from the run; decoded copies are labelled separately.
