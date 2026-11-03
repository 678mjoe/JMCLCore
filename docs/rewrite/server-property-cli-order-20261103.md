# Ordered server property CLI inputs

Implementation checkpoint: `721055d12ba3e63c7e11ab91e7abbcb6dee7943e`.
Pinned original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

## Corrections

The CLI previously accepted unknown flag-shaped assignments such as
`--bogus=value` and more than 1024 input pairs when duplicate keys collapsed in
its Map. A pure red regression confirmed the invalid flag was accepted.

More importantly, Map construction discarded early duplicate values before
validation. An oversized motd followed by motd=ok could bypass the original
per-occurrence rejection. The parser now keeps an internal ordered sidecar,
checks the raw pair count and rejects unknown -- prefixes. It deliberately does
not validate property value limits early.

Only the CLI-specific internal entry passes the sidecar. All ordinary RPC calls
still pass None and use the original object contract. Ordered application stays
after root, manifest, server directory, stopped lock and property-file loading,
so it does not outrank existing earlier errors. Every occurrence reaches the
existing Document::set validator; the original sole atomic write remains after
all pairs succeed. Failed intermediate changes affect only the in-memory copy.

## Evidence and limits

- Three new pure input/document tests passed: exact 1024/1025 raw-pair boundary,
  duplicate pairs, flag-shaped assignments, directories excluded from pair count,
  invalid early/late duplicate values and valid last-value/first-position behavior.
- Three existing property-codec tests and four aggregate pure numeric tests passed.
- Four malformed CLI cases match pinned Zig exact status/stdout/stderr, with no
  roots, properties or other files created. They stop at syntax rejection.
- Offline locked build, warning-denied all-target Clippy, fmt and static audit
  regressions passed (22 normal and 22 optimized).
- A compile error from an initially overbroad signature edit was corrected before
  acceptance. Source review confirmed only handle_inner retains the new parameter.

Only Linux executed. No valid server-property write, lifecycle operation, service,
network, EULA acceptance, Java or archive was exercised. Ordered real filesystem
publication remains unexecuted; pure tests and reviewed control flow do not claim
that end-to-end acceptance. No remote branch changed.
