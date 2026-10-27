# Private plan component byte seam

Source: Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/install/plan.zig:558` (`validateComponent`), compared with Rust
`plan::validate_component` at recovery checkpoint `18c2f261`.
This is source-derived policy evidence; Zig was not executed.

| Input/rule, in source order | General plan component | Loader profile ID |
| --- | --- | --- |
| Empty / exact `.` / exact `..` | `UnsafePath` | `InvalidProfileId` (empty or trailing dot) |
| Trailing ASCII dot or space | `UnsafePath` | `InvalidProfileId` |
| ASCII controls, DEL, slash, backslash, colon | `UnsafePath` | `InvalidProfileId` |
| Stem before first dot, ASCII case-insensitive CON/PRN/AUX/NUL/COM1–9/LPT1–9 | `UnsafePath` | No device-stem rule |
| Unicode UTF-8 / opaque high bytes | Permitted unless another rule fails | Rejected by ASCII allowlist |
| Length >256 bytes | No component cap here | Rejected |
| Other punctuation | Not rejected by this component policy alone | Only ASCII alphanumeric and `-_+.` |

Loader column refers to pinned `loaders/fabric.zig:259` and Rust
`version_fabric.rs::profile_id`. NeoForge version IDs use equivalent ASCII
rules with `InvalidNeoForgeVersion`; Forge maps its loader version error to
`InvalidForgeVersion`. Maven coordinate components have their own ASCII rules
and `InvalidMavenCoordinate`. None are changed or delegated to this helper.
General plan components cover game version IDs, asset index IDs, logging file
IDs and relative-path segments. They are not an archive-prefix policy.

The baseline discrepancy is representational: Rust's public `&str` validator
cannot receive `[0x80]` or `b"v\xff"`, while Zig's byte rule permits them. There
was no demonstrated mismatch on the existing UTF-8 domain. The private byte
helper retains the same check sequence and error; the public String validator
now delegates with `as_bytes()`. ASCII splitting, uppercasing and prohibited
byte checks have identical behavior on UTF-8. All rejected cases return the
same `UnsafePath`, so tests cannot distinguish internal precedence among these
checks; sequence equivalence is established by source review.

No path is constructed by these tests. Source policy also permits punctuation
such as `*?<>|"`; the previous Rust policy already did so. This must not be
interpreted as Windows filename validity or used to relax downstream guards.
No security guard was removed. Native conversion/platform handling remains
separate, as do archive/store policies.

The helper is live through the existing String validator, not unused code.
The next opaque consumer would be a separately reviewed byte-aware plan input
and path representation at `plan.rs::build_with_extras` (version ID validation
before client path construction), then asset/logging IDs. Current public
`Plan.version_id`, path Strings, metadata operational parser and install/launch
APIs still cannot carry opaque IDs. They remain unchanged; this seam alone
does not fix operational planning or establish native platform support.
