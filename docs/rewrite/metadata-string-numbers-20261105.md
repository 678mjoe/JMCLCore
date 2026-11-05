# Shared dynamic numeric string parsing

Local continuation from server-number fix `f5b7d82`; not published and not part of
the four-platform CI source `190be14`.

The same incorrect binary64 string fallback remained in `metadata::unsigned`.
This helper is used by version-list limits, install parameter counts and byte
arrays, and modpack schema adapters. Its string branch now uses the shared typed
unsigned parser, mapping its error category and retaining the existing caller
maximum check. Actual JSON Number behavior is unchanged.

The new direct regression failed before the fix on `"1_0"`. Tests cover internal
underscores, hexadecimal floats, exact integers beyond binary64 precision,
u64 maximum, fractional strings, invalid syntax and the existing Number behavior.

Additional pure call-site regressions only deserialize `ExecuteParams` and
`PathValue`, or call `validate_params` for version limits. They do not invoke
install/version handlers, transport, planner, filesystem or service operations.
The actual Zig standard-library string conversion was independently checked by
`rust/compat/dynamic-string-numbers-oracle.zig` in the prior commit.

Unquoted numbers in typed modpack schemas are a separate precision question and
are not claimed fixed here. No acquisition, archive, Java, network or running
server operation is tested by this change.

Validation: 20 metadata tests, four server-number tests, two shared JSON-number
tests and the two new exact pure call-site tests passed. Formatting and all-target
Clippy pass. Independent review confirmed no production routing change. Only the
metadata source and test-only install RPC source audit fingerprints are refreshed.
