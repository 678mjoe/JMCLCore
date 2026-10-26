# Instance create: pure UTF-8 parameter grammar

Pinned `src/protocol/instance.zig:8–18` defines eight byte-string fields,
including `source`. Source is **not** a typed enum here: enum spelling is
checked later by the handler. Required directory/ID/version fields, optional
name/loader fields, default `source="official"` and `validate=true` retain
those distinct grammar rules.

The private Rust Create decoder accepts UTF-8 strings or dynamic-u8 arrays
through the existing lexical number helper, with checked UTF-8 conversion.
Only Create gains an explicit object-shape guard. The generic parser and
handler body after parsing remain unchanged: range, ID, source spelling and
loader-count checks still precede resolution and root creation. No public
operational Manifest or resolver type is widened by this slice.

## Pure evidence, not handler execution

Baseline commit `eff3e7d` and its retained evidence record a strict 53-case
pure diagnostic with 29 differences. The final fix does not register that
intentionally failing historical test. The std-only Zig adapter's declaration
must match clean pinned source (line endings normalized). No Core import,
handler, resolver, root or create function is invoked; all rows log complete
input/source/Rust values.

The scoped acceptance test is
`instances::create_params_tests::pinned_create_utf8_parameter_schema_only`:
44 cases require strict source equality; eight opaque-array fields retain
explicit, individually asserted unsupported outcomes. A ninth residual case
is the helper's ignored lone-surrogate behavior; the actual outer RPC guard
already rejects it. No reachable RPC defect is claimed for that row. The full
53 rows remain in this gate with exact residual identities and
values; there is no aggregate-test skip or intentionally failing registration.

`instances::create_params_tests::create_parameter_defaults_and_optional_nulls`
checks the default source/validation flag, all four optional null fields,
and rejection of null required fields/source/validation. These tests do not
call the handler and therefore establish no creation or resolver acceptance.

## Explicit remaining consumer scope

Opaque native directory paths require the checked native-path root bridge.
ID and source spelling have byte-exact semantic restrictions, but display
name, version and loader versions are not generally UTF-8 constrained by the
original manifest validator: it checks byte lengths. With `validate=false`,
those bytes may be stored successfully after root opening. Full closure needs
a separate byte-native manifest writer/consumer slice with original error
ordering and disk effects; rejecting opaque bytes early is not equivalent.
The `validate=true` resolver boundary also needs its own evidence. Neither is
permanently blocked merely because this pure UTF-8 slice is narrower.

Local Linux evidence: the exact oracle gate and two pure decoder controls
pass; static library/tests Clippy with warnings denied passes. All 53 rows
are retained, with 20 baseline differences corrected. No
handler, cache, root, metadata resolver or creation command was invoked.
Native execution remains subject to the exact-tree CI gate.
