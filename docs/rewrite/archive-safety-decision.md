# Approved narrow archive safety exception

On 2026-10-24 the project owner approved preserving legitimate internal archive
links while rejecting links that would allow extraction writes to escape the
staging directory. This is an explicit security compatibility exception: rejecting
such unsafe archives may differ from the pinned Zig implementation's acceptance
behavior. The boundary concerns where extraction may write, not whether a path
contains a symbolic link in the abstract.

## What remains compatible

- Legitimate internal archive links remain part of the intended supported behavior.
- Previously verified read-only aliases such as `instance.json` and `level.dat`
  remain supported. This decision is not a blanket symbolic-link ban.
- Normal protocol, CLI and disk behavior still follows the pinned reference.
  Unrelated differences, including ZIP CRC acceptance, are not automatically
  defects or covered by this archive-specific exception. Confirmed original
  defects are tracked separately in the [defect ledger](original-core-defects.md).
- This decision does not authorize changes to Zig `main` or a mainline switch.

## Implementation and verification status

The managed Java runtime archive installer remains incomplete, and its further
verification is blocked. The approved boundary is a requirement, not evidence
that the implementation already enforces it. Existing local-Java detection,
selection and execution tests do not establish archive-installation correctness.

Future acceptance must separately establish confinement, preservation of valid
internal links, cleanup and publication behavior, and relevant platform behavior.
The exception must remain explicit in compatibility records rather than being
silently normalized in differential tests. It does not remove the other
[replacement gates](README.md).
