# Verification URI decimal port spelling

Based on ebe8129. The private HTTPS predicate for device authorization response
verification URIs now parses a port using the shared Zig-compatible unsigned
decimal parser and checked u16 conversion. HTTPS scheme, userinfo rejection,
hostname decoding/validation, response fields and dispatch routes are unchanged.
This does not change request endpoints or initiate authentication.

The pure regression first failed on port 4_43. Rust and the pinned Zig pure URI
predicates cover 19 cases: internal/repeated underscores, plus, negative zero,
u16 edges, invalid numeric syntax, and unchanged scheme/userinfo/host rejection.
The Zig fixture copies only the two original pure URI predicates, using the
pinned standard library; it does not import or run an authentication service.

Local tests are pure predicate calls, formatting, all-target Clippy, and static
routing/guard checks. No credential, network, installation, Java or archive work.
The audit update changes only the reviewed auth source digest and commit anchor.
Remote branches and the bounded CI test list are unchanged.
