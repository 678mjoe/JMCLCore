# Version parameter UTF-8 byte arrays

Separate follow-up to enum slice `635c842`. The same private parameter view
canonicalizes declared `id` (get/resolve/plan) and nullable `fabric_loader`,
`neoforge_version`, `forge_version` (resolve/plan) byte arrays into Strings only
when the bytes are valid UTF-8. Ordinary strings remain borrowed and unchanged.
No public Service API, execution body, metadata type, transport, or planner
changes. No provider or archive operation is part of the evidence.

Each array element uses dynamic-u8 semantics: raw Value floats have already
rounded to binary64, whereas quoted strings retain exact typed-number parsing.
There is no `as u8` truncation before range validation. Entire declared-field
schema preparation precedes existing empty-ID and loader-count guards. Optional
nulls, absent defaults and method-irrelevant/unknown fields remain unchanged.

## Explicit remaining boundary

The original 106 source/helper rows are retained. The intended gate is 101
matching rows and the exact five opaque-byte discrepancies: ID `[255]` for
get/resolve/plan, Fabric loader `[255]` for resolve/plan. They remain rejected by
Rust and accepted by the source request schema; this is **unresolved**, not an
approved deviation or a claim that the source disallows opaque bytes.

Source `service.zig:1196` selects manifest IDs by byte equality after fetching
its manifest. Fabric loader validation follows parent resolution, while NeoForge
and Forge validate their loader before parent resolution. A later byte-aware
consumer seam must retain these distinctions; it must not replace opaque input
with lossy text, silently force VersionNotFound, or move late errors to params.

## Pure test allowlist (no handlers)

- `version::params_contract_tests::version_parameter_utf8_bytes_match_pinned_dynamic_schema`
  (ignored pinned-source oracle; retains all 106 rows).
- `version::params_contract_tests::utf8_byte_parameters_preserve_dynamic_grammar_and_schema_order`
  (actual parse/prepare/validate, exact errors/values).
- `version::params_contract_tests::enum_schema_precedes_semantic_guards_without_mutating_input`
  (prior enum/error/default regression).

Local Linux verification: each of the three named tests executed exactly once
and passed; the full 106 rows contain 101 exact matches and five explicit opaque
byte discrepancies. Static all-target Clippy with `-D warnings` passed. Historical
baseline and enum-stage raw results/test ELFs remain retained separately. No
claim of native or handler verification follows from these helper tests.

Test ELF SHA-256:
`bb364204fe8a46feb4bef80c0a9f92a7216baf830a0e452d56530da34038f8a2`.
