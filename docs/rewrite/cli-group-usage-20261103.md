# Unknown command-group diagnostics

Implementation checkpoint: `fe5ff751d00481a578157e6c77868bd3e3272002`.
Pinned original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

Pure red comparisons found three bounded defects: instance without a known
subcommand returned a rewrite-specific message; install without a known subcommand
fell through to unknown-command text; opaque unknown instance subcommands panicked
at String argv conversion. The existing launch String route was already correct.

The String fallback now prints the original group usage after existing handlers
have had their chance. A pure native diagnostic branch recognizes only unknown
second arguments for instance/install/launch, and preserves the total non-UTF-8
argv gate. Every known subcommand returns None so existing operational routes and
byte bridges retain precedence. No tail argument can rescue an unknown second
argument. No handler is invoked by this classification.

Evidence: four library diagnostic tests and one binary-dispatch test passed.
Twelve String and eighteen POSIX opaque-argument cases match exact original
status/stdout/stderr with no writes. Existing seven finite corpora were rerun on
the same binary, totaling 164 paired cases. Two explicitly owned instance-read
cases create their expected empty roots; every other tree remains unchanged.
Binary SHA256: `ae86bcc5da6c946039e7a0add63533a2a5bcca74bdd41b6cb4ef433a17a983ed`.
Offline locked build, all-target warning-denied Clippy, fmt and static audit
regressions passed (22 normal and 22 optimized).

Linux only; no native Windows/macOS, valid installation, Java, network, service
or game execution was performed. Known operational raw-argument gaps remain.
This change is local and not included in the separately prepared CI proposal,
which deliberately pins the previously saved remote commit 495b35b. Neither
new work branch has been remotely published.
