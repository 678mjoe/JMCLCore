# Percent-decoded verification hostname bound

Continuation from 659fcde. Zig Uri.Component.toRaw uses a 255-byte output buffer
when the host contains a percent sign. Unescaped hosts return their original
slice instead. Rust now applies the same bound after percent decoding but before
removing a trailing dot, only when the input host contains a percent sign.
The ordinary 255-byte host and 256-byte host with trailing dot remain accepted.

A four-case regression failed before the change on an escaped 256-byte hostname
with a trailing dot. Both pure Rust tests now pass; the Zig predicates verify
the same boundary and all 19 port/scheme/userinfo cases. No authentication or
network request is made.

The standalone corpus in rust/compat/auth-uri-pure-corpus extracts the Rust pure
predicate from the final source, removing only its Zeroizing allocation wrapper.
Its Zig adapter copies the pinned original two pure predicates. 17,590 synthetic
URIs (7,590 structured combinations and 10,000 seeded random strings) produced
zero differences. This is finite pure predicate coverage, not full Core parity.

Candidate binary SHA256:
e7392fb70eaf10b22005ed0b9f8fe9ce76918b184418b78b58be075d0b201410
Oracle adapter binary SHA256:
ac72ed0c9312162f461b18ad41d7f736ca23eef4db9e3112b0054965539c080c

Formatting and all-target Clippy pass. Routing audit only re-pins the reviewed
auth source and commit anchor; 28 static guards pass in each Python mode.
No remote changes, CI scope expansion, platform verification, Java or archive
work is included.
