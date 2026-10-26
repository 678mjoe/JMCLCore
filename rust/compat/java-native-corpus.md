# Native explicit-path Java probe/selection corpus

`java.py` now compiles the existing property-printing Rust fixture as a real
host executable (including PE on Windows), rather than omitting successful
Windows detect/select cases or treating a shell script as an `.exe`. It is not
a JVM and cannot launch a game. The fixture rejects inherited Java option
environment variables, so the positive result also checks option sanitization.

The explicit fixture advertises Java 17 / x86_64 on every host. Tests assert that
exact executable and metadata, successful selection for explicit x86_64, and
`JAVA_INCOMPATIBLE` for arm64. This is explicit-architecture selection evidence,
not default-host architecture or real-runtime acceptance. Existing Unix invalid
executable/permission cases remain Unix-only.

A previously masked coverage defect was found: the old cache fixture omitted
required client download metadata, so apparently successful selection scenarios
only compared identical `METADATA_ERROR` responses. The cache is now a valid
synthetic metadata document, and every positive selection requires a result with
the expected runtime, major version and architecture. A negative control ensures
that `METADATA_ERROR` cannot satisfy this assertion. Artifact URLs are only
metadata: selection does not fetch client artifacts or acquire Java.

The script still uses the same pinned Core oracle and exact RPC/CLI byte/status
comparison. Both official and mirror source paths are pre-seeded in the local
cache. No provider requests, automatic acquisition, archives or production Java
implementation are added by this test-only change.
